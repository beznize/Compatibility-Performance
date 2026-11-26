//! Match the engine's recorded state and shader identities before creating a host PSO.
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result};
use victory_game::{pipelines, sgd5, shaderpack};
use victory_hle::modules::d3d12x::{audit, warm_graphics};
use victory_shader::{Container, FourCC, HwStage, native_programs};

struct Shader {
    data: Vec<u8>,
    digest: blake3::Hash,
    stage: HwStage,
}

#[derive(Default)]
struct Index {
    shaders: Vec<Shader>,
    database: HashMap<(usize, u64), Vec<usize>>,
    native: HashMap<(HwStage, [u8; 24]), Vec<usize>>,
    // Pairs actually declared by techniques, indexed by the engine's vertex hash.
    pixel_for_vertex: HashMap<u64, HashSet<usize>>,
}

fn add<K: std::hash::Hash + Eq>(map: &mut HashMap<K, Vec<usize>>, k: K, i: usize) {
    let ids = map.entry(k).or_default();
    if !ids.contains(&i) {
        ids.push(i);
    }
}

impl Index {
    fn new(databases: &[sgd5::Database<'_>], extra: &External) -> Self {
        let mut index = Self::default();
        let mut unique = HashMap::new();
        for db in databases {
            for (slot, list) in db.shaders.iter().enumerate() {
                for shader in list {
                    let load = || -> Option<(Vec<u8>, blake3::Hash)> {
                        let mut bytes = shader.record.unpack()?;
                        let size = u32::from_le_bytes(bytes.get(24..28)?.try_into().ok()?) as usize;
                        bytes.get(..size)?;
                        bytes.truncate(size);
                        let digest = blake3::hash(&bytes);
                        Some((bytes, digest))
                    };
                    let Some((bytes, digest)) = load() else {
                        audit::event(
                            "database",
                            "missing",
                            "-",
                            format_args!(
                                "stage {slot} hash {:016x}: unreadable container",
                                shader.hash
                            ),
                        );
                        continue;
                    };
                    let i = if let Some(&i) = unique.get(&digest) {
                        i
                    } else {
                        let native = Container::parse(&bytes)
                            .ok()
                            .and_then(|c| native_programs(c.part(FourCC::DXIL)?.data).ok());
                        let Some(p) = native.as_deref().and_then(|programs| {
                            programs.iter().find(|p| {
                                matches!(
                                    (slot, p.stage),
                                    (0, HwStage::Vs | HwStage::Ms)
                                        | (1, HwStage::Ps)
                                        | (5, HwStage::Cs)
                                )
                            })
                        }) else {
                            audit::event(
                                "database",
                                "missing",
                                &digest.to_hex(),
                                format_args!(
                                    "unresolved database stage {slot} {:016x} {}",
                                    shader.hash,
                                    shader.record.name.unwrap_or("?")
                                ),
                            );
                            continue;
                        };
                        let i = index.shaders.len();
                        if let Some(id) = shaderpack::program_id(&p.code) {
                            add(&mut index.native, (p.stage, id), i);
                        }
                        index.shaders.push(Shader {
                            data: bytes,
                            digest,
                            stage: p.stage,
                        });
                        unique.insert(digest, i);
                        i
                    };
                    add(&mut index.database, (slot, shader.hash), i);
                    if audit::enabled() {
                        audit::event(
                            "database",
                            "shader",
                            &digest.to_hex(),
                            format_args!(
                                "{slot} {:016x} {}",
                                shader.hash,
                                shader.record.name.unwrap_or("?")
                            ),
                        );
                    }
                }
            }
            for effect in &db.effects {
                for technique in &effect.techniques {
                    for pass in &technique.passes {
                        if let (Some(v), Some(p)) = (pass.shaders[0], pass.shaders[1]) {
                            let vertex = db.shaders[0][v].hash;
                            let pixel = db.shaders[1][p].hash;
                            if let Some(ids) = index.database.get(&(1, pixel)) {
                                index
                                    .pixel_for_vertex
                                    .entry(vertex)
                                    .or_default()
                                    .extend(ids);
                            }
                        }
                    }
                }
            }
            if audit::enabled() {
                for effect in &db.effects {
                    for technique in &effect.techniques {
                        for (pass, refs) in technique.passes.iter().enumerate() {
                            for (slot, s) in refs.shaders.iter().enumerate() {
                                if let Some(shader) = s.and_then(|i| db.shaders[slot].get(i)) {
                                    audit::event(
                                        "effect",
                                        "declared",
                                        &format!("{:016x}", shader.hash),
                                        format_args!(
                                            "{slot} {} {} {pass}",
                                            effect.name, technique.name
                                        ),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        for (digest, (bytes, stage, native)) in &extra.containers {
            let i = *unique.entry(*digest).or_insert_with(|| {
                let i = index.shaders.len();
                index.shaders.push(Shader {
                    data: bytes.clone(),
                    digest: *digest,
                    stage: *stage,
                });
                i
            });
            add(&mut index.native, (*stage, *native), i);
        }
        index
    }

    fn unique(
        &self,
        ids: Option<&Vec<usize>>,
        missing: &'static str,
    ) -> Result<&Shader, &'static str> {
        match ids.map(Vec::as_slice) {
            Some([i]) => Ok(&self.shaders[*i]),
            Some([_, _, ..]) => Err("ambiguous shader container identity"),
            _ => Err(missing),
        }
    }
}

/// Standalone containers outside the effect database can implement built-in pack records.
#[derive(Default)]
pub(crate) struct External {
    containers: HashMap<blake3::Hash, (Vec<u8>, HwStage, [u8; 24])>,
}

impl External {
    pub(crate) fn remember(&mut self, bytes: &[u8], programs: &[victory_shader::NativeProgram]) {
        if let Some((p, id)) = programs
            .iter()
            .filter(|p| {
                matches!(
                    p.stage,
                    HwStage::Vs | HwStage::Ps | HwStage::Ms | HwStage::Cs
                )
            })
            .find_map(|p| shaderpack::program_id(&p.code).map(|id| (p, id)))
        {
            self.containers
                .entry(blake3::hash(bytes))
                .or_insert_with(|| (bytes.to_vec(), p.stage, id));
        }
    }

    fn read_image(&mut self, path: &Path) -> Result<()> {
        let image = crate::archive::Archive::open(path)?;
        for src in crate::shader::sources(&image) {
            if let Some(bytes) = src.bytes()
                && let Ok(c) = Container::parse(&bytes)
                && let Some(part) = c.part(FourCC::DXIL)
                && let Ok(native) = native_programs(part.data)
                && let Some(extent) = bytes
                    .get(24..28)
                    .and_then(|d| d.try_into().ok())
                    .map(u32::from_le_bytes)
                && let Some(bytes) = bytes.get(..extent as usize)
            {
                self.remember(bytes, &native);
            }
        }
        Ok(())
    }
}

pub fn run(path: &Path, catalog_only: bool, jobs: usize) -> Result<()> {
    let bytes =
        crate::archive::Archive::open(path).with_context(|| format!("read {}", path.display()))?;
    let packs = shaderpack::find_all(&bytes, |offset| {
        audit::event(
            "scan",
            "invalid_pack_signature",
            "-",
            format_args!("{} {offset:#x}", path.display()),
        );
    });
    anyhow::ensure!(!packs.is_empty(), "no valid engine shader pack found");
    let mut extra = External::default();
    // In this standalone mode, also look for the engine's built-in shaders
    // inside sibling program images. Full warm-up indexes every input file.
    for file in std::fs::read_dir(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )? {
        let path = file?.path();
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe") || ext.eq_ignore_ascii_case("dll"))
        {
            extra.read_image(&path)?;
        }
    }
    for (offset, pack) in packs {
        audit::event(
            "scan",
            "pack",
            "-",
            format_args!("{} {offset:#x}", path.display()),
        );
        warm(&bytes, &pack, &extra, !catalog_only, jobs)?;
    }
    Ok(())
}

pub fn warm(
    archive: &[u8],
    data: &[u8],
    extra: &External,
    create: bool,
    jobs: usize,
) -> Result<()> {
    let pack = shaderpack::Pack::parse(data).map_err(anyhow::Error::msg)?;
    let dbs = sgd5::databases_checked(archive, |at| {
        audit::event("database", "invalid_signature", "-", at)
    });
    let named = pipelines::Named::find(&pack, &dbs);
    let index = Index::new(&dbs, extra);
    let graphics: Vec<_> = pack.graphics().collect();
    let mut compute = HashSet::new();
    let mut unknown_compute = 0;
    for record in pack.compute() {
        let ids = index.database.get(&(5, record.shader_hash)).or_else(|| {
            index
                .native
                .get(&(HwStage::Cs, shaderpack::program_id(record.stage.program)?))
        });
        match index.unique(ids, "no matching compute container") {
            Ok(s) => {
                compute.insert(s.digest);
                audit::event(
                    "pack-compute",
                    "identified",
                    &s.digest.to_hex(),
                    format_args!("{:016x}", record.shader_hash),
                );
            }
            Err(reason) => {
                unknown_compute += 1;
                audit::event(
                    "pack-compute",
                    "missing",
                    &format!("{:016x}", record.shader_hash),
                    reason,
                );
            }
        }
    }
    println!(
        "pack         {} graphics records, {} compute records, {} distinct matched compute containers, {unknown_compute} unresolved compute records",
        graphics.len(),
        pack.compute().count(),
        compute.len()
    );
    let [next, matched, made, rejected, unresolved] = [const { AtomicUsize::new(0) }; 5];
    std::thread::scope(|scope| {
        for _ in 0..jobs.max(1).min(graphics.len()) {
            scope.spawn(|| {
                while let Some(g) = graphics.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let resolve = || -> Result<_, String> {
                        let first = if let Some(ids) = index.database.get(&(0, g.shader_hash)) {
                            index.unique(Some(ids), "no vertex container")?
                        } else {
                            let mut ids = Vec::new();
                            for stage in g.stages[1..].iter().flatten() {
                                if let Some(id) = shaderpack::program_id(stage.program) {
                                    for ty in [HwStage::Vs, HwStage::Ms] {
                                        if let Some(found) = index.native.get(&(ty, id)) {
                                            ids.extend_from_slice(found);
                                        }
                                    }
                                }
                            }
                            ids.sort_unstable();
                            ids.dedup();
                            index.unique(Some(&ids), "no matching vertex or mesh container")?
                        };
                        let ps = g.stages[0]
                            .as_ref()
                            .map(|p| {
                                let id = shaderpack::program_id(p.program)
                                    .ok_or("no pixel program identity")?;
                                let candidates = index.native.get(&(HwStage::Ps, id));
                                if let Some(ids) = candidates
                                    && ids.len() > 1
                                {
                                    let pairs = index.pixel_for_vertex.get(&g.shader_hash);
                                    let mut declared = ids
                                        .iter()
                                        .filter(|i| pairs.is_some_and(|pairs| pairs.contains(i)));
                                    if let (Some(&i), None) = (declared.next(), declared.next()) {
                                        return Ok(&index.shaders[i]);
                                    }
                                }
                                index.unique(candidates, "no matching pixel container")
                            })
                            .transpose()?;
                        let stream = pipelines::stream(
                            g,
                            &named,
                            &first.data,
                            first.stage == HwStage::Ms,
                            ps.map(|p| p.data.as_slice()),
                        )
                        .map_err(|e| format!("unresolved render state {e:?}"))?;
                        Ok((stream, first, ps))
                    };
                    match resolve() {
                        Ok((stream, first, ps)) => {
                            matched.fetch_add(1, Ordering::Relaxed);
                            if audit::enabled() {
                                audit::event(
                                    "pack-graphics",
                                    "identified",
                                    &format!("{:016x}", g.key),
                                    format_args!(
                                        "{:?}={} pixel={}",
                                        first.stage,
                                        first.digest,
                                        ps.map_or_else(|| "-".into(), |s| s.digest.to_string())
                                    ),
                                );
                            }
                            if create {
                                // SAFETY: the stream owns its input layout; its shader bytes stay in the index.
                                let success = unsafe { warm_graphics(stream.bytes(), g.key) };
                                (if success { &made } else { &rejected })
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        Err(why) => {
                            unresolved.fetch_add(1, Ordering::Relaxed);
                            audit::event(
                                "pack-graphics",
                                "missing",
                                &format!("{:016x}", g.key),
                                why,
                            );
                        }
                    }
                }
            });
        }
    });
    println!(
        "pack         {} graphics records matched, {} host pipelines created, {} rejected, {} unresolved",
        matched.into_inner(),
        made.into_inner(),
        rejected.into_inner(),
        unresolved.into_inner()
    );
    audit::flush();
    Ok(())
}
