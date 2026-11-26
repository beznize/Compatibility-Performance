use std::sync::Mutex;
#[cfg(feature = "diagnostics")]
use std::sync::atomic::{AtomicU64, Ordering};
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::GUID;

// A sealed heap is released when its last texture dies. Individual ranges
// are never reused, so freshly streamed textures cannot see old tile data.
const HEAP_BYTES: u64 = 256 << 10;
const OWNER: GUID = GUID::from_u128(0x7f73d445_03a1_4c53_9db6_3fdf9294d772);
struct Page {
    device: ID3D12Device,
    heap: ID3D12Heap,
    used: u64,
}
static CURRENT: Mutex<Option<Page>> = Mutex::new(None);

/// Pack small read-only 2D textures at the placement alignment reported by
/// the device. The caller uses committed resources for all other layouts.
pub(super) fn place(device: &ID3D12Device, desc: &D3D12_RESOURCE_DESC, state: D3D12_RESOURCE_STATES) -> Option<ID3D12Resource> {
    if desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D || desc.DepthOrArraySize != 1
        || desc.SampleDesc.Count != 1 || desc.Flags != D3D12_RESOURCE_FLAG_NONE
        || desc.Layout != D3D12_TEXTURE_LAYOUT_UNKNOWN
    {
        return None;
    }
    let (block_bytes, block) = super::element(desc.Format.0 as u32)?;
    let top = desc.Width.div_ceil(block).checked_mul(u64::from(desc.Height).div_ceil(block))?.checked_mul(block_bytes)?;
    if top > D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT as u64 {
        return None;
    }
    let small = D3D12_RESOURCE_DESC { Alignment: D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT as u64, ..*desc };
    // SAFETY: the device determines eligibility and includes every mip.
    let info = unsafe { device.GetResourceAllocationInfo(0, &[small]) };
    if info.Alignment != small.Alignment || info.SizeInBytes == 0 || info.SizeInBytes >= D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT as u64 {
        return None;
    }
    let mut current = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    if current.as_ref().is_none_or(|p| &p.device != device || p.used.next_multiple_of(info.Alignment) + info.SizeInBytes > HEAP_BYTES) {
        let mut heap = None;
        let description = D3D12_HEAP_DESC {
            SizeInBytes: HEAP_BYTES,
            Properties: D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, CreationNodeMask: 1, VisibleNodeMask: 1, ..Default::default() },
            Flags: D3D12_HEAP_FLAG_ALLOW_ONLY_NON_RT_DS_TEXTURES,
            ..Default::default()
        };
        // SAFETY: a default, zero-initialized heap for sampled textures.
        unsafe { device.CreateHeap(&description, &mut heap).ok()? };
        *current = Some(Page { device: device.clone(), heap: heap?, used: 0 });
    }
    let page = current.as_mut()?;
    let offset = page.used.next_multiple_of(info.Alignment);
    let mut texture: Option<ID3D12Resource> = None;
    // SAFETY: the verified range is aligned and has never been handed out.
    unsafe { device.CreatePlacedResource(&page.heap, offset, &small, state, None, &mut texture).ok()? };
    let texture = texture?;
    // The resource can outlive its Rust wrapper in a descriptor or queued
    // command list. Attach the heap to the COM resource's actual lifetime.
    unsafe { texture.SetPrivateDataInterface(&OWNER, &page.heap).ok()? };
    page.used = offset + info.SizeInBytes;
    drop(current);
    #[cfg(feature = "diagnostics")]
    {
        static COUNT: AtomicU64 = AtomicU64::new(0);
        static BYTES: AtomicU64 = AtomicU64::new(0);
        let bytes = BYTES.fetch_add(info.SizeInBytes, Ordering::Relaxed) + info.SizeInBytes;
        let count = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        if count == 1 || count.is_multiple_of(4096) {
            tracing::info!(count, bytes, "small streamed textures placed in shared heaps");
        }
    }
    Some(texture)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R32_UINT, DXGI_SAMPLE_DESC};

    #[test]
    fn small_textures_keep_their_bytes_after_the_heap_rolls_over() {
        let Some(host) = crate::host::host() else { return };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D, Width: 1, Height: 1,
            DepthOrArraySize: 1, MipLevels: 1, Format: DXGI_FORMAT_R32_UINT,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 }, ..Default::default()
        };
        let list = host.new_list().unwrap();
        let count = 1 + HEAP_BYTES / D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT as u64;
        let stride = D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT as u64;
        let (source, data) = list.upload(host, count * stride).unwrap();
        let dest = host.read_back_buffer(count * stride).unwrap();
        let mut textures = Vec::new();
        let texel = |i: u64| 0x723a_b148u32.wrapping_mul(i as u32 + 1);
        let buffer = |resource: &ID3D12Resource, i| D3D12_TEXTURE_COPY_LOCATION {
            pResource: unsafe { std::mem::transmute_copy(resource) },
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                Offset: i * stride, Footprint: D3D12_SUBRESOURCE_FOOTPRINT { Format: desc.Format, Width: 1, Height: 1,
                    Depth: 1, RowPitch: D3D12_TEXTURE_DATA_PITCH_ALIGNMENT }
            }},
        };
        for i in 0..count {
            let texture = place(&host.device, &desc, D3D12_RESOURCE_STATE_COPY_DEST).unwrap();
            let location = D3D12_TEXTURE_COPY_LOCATION {
                pResource: unsafe { std::mem::transmute_copy(&texture) },
                Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
            };
            // All resources and both transfer buffers stay alive through the fence.
            unsafe {
                data.add((i * stride) as usize).cast::<u32>().write_unaligned(texel(i));
                list.list.CopyTextureRegion(&location, 0, 0, 0, &buffer(&source, i), None);
                list.list.ResourceBarrier(&[crate::host::transition(&texture, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
                list.list.CopyTextureRegion(&buffer(&dest, i), 0, 0, 0, &location, None);
            }
            textures.push(texture);
        }
        list.close();
        host.execute(&[&list]);
        let mut bytes = std::ptr::null_mut();
        unsafe { dest.Map(0, None, Some(&mut bytes)).unwrap() };
        for i in 0..count {
            assert_eq!(unsafe { bytes.cast::<u8>().add((i * stride) as usize).cast::<u32>().read_unaligned() }, texel(i));
        }
        unsafe { dest.Unmap(0, Some(&D3D12_RANGE::default())) };
        let writable = D3D12_RESOURCE_DESC { Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, ..desc };
        assert!(place(&host.device, &writable, D3D12_RESOURCE_STATE_COMMON).is_none());
    }
}
