//! Compare offline discoveries, guest PSO requests and fence-confirmed GPU work.
use anyhow::{Context, Result, bail, ensure};
use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Pipeline {
    offline: u64,
    offline_created: u64,
    offline_failed: u64,
    requested: u64,
    created: u64,
    rejected: u64,
    skipped: u64,
    pending: u64,
    recorded: u64,
    missing: u64,
    submitted: u64,
    finished: u64,
    stages: String,
}

#[derive(Default)]
struct Coverage {
    pipelines: BTreeMap<String, Pipeline>,
    offline_shaders: HashSet<String>,
    translated_shaders: HashSet<String>,
    rejected_shaders: HashSet<String>,
    other_containers: HashSet<String>,
    runtime_shaders: HashSet<String>,
    counts: BTreeMap<(String, String), u64>,
    first_party_files: u64,
    scanned_bytes: u64,
}

impl Coverage {
    fn read(&mut self, path: &Path, offline: bool) -> Result<()> {
        let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
        self.read_journal(BufReader::new(file), path, offline)
    }

    fn read_journal(&mut self, input: impl BufRead, path: &Path, offline: bool) -> Result<()> {
        let mut lines = input.lines();
        ensure!(
            lines.next().transpose()?.as_deref()
                == Some("sequence\tframe\tsource\tevent\tidentity\tdetail"),
            "unexpected audit header in {}",
            path.display()
        );
        // Fence values start afresh in each process. A submitted command is only
        // covered by a non-sentinel completion from that same journal.
        let mut finished = 0;
        let mut submissions = Vec::new();
        for (i, line) in lines.enumerate() {
            let line = line?;
            let mut fields = line.splitn(6, '\t');
            let (Some(seq), Some(_), Some(source), Some(event), Some(key), Some(detail)) = (
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
                fields.next(),
            ) else {
                bail!("incomplete audit row {} in {}", i + 2, path.display());
            };
            ensure!(
                seq.parse::<usize>()? == i + 1,
                "audit sequence gap in {}",
                path.display()
            );
            *self
                .counts
                .entry((source.into(), event.into()))
                .or_default() += 1;
            match (source, event) {
                ("scan", "opened") if offline => {
                    self.first_party_files += 1;
                    self.scanned_bytes += detail
                        .split_whitespace()
                        .next()
                        .context("missing file size")?
                        .parse::<u64>()?;
                }
                ("offline-shader", "discovered") if offline => {
                    self.offline_shaders.insert(key.into());
                }
                ("offline-shader", "translated") if offline => {
                    self.translated_shaders.insert(key.into());
                }
                ("offline-shader", "rejected") if offline => {
                    self.rejected_shaders.insert(key.into());
                }
                ("offline-shader", "unimplemented") if offline => {
                    self.other_containers.insert(key.into());
                }
                ("queue", "completed_through") if !offline => {
                    let f: u64 = detail.parse()?;
                    ensure!(f != u64::MAX, "invalid completion fence");
                    finished = finished.max(f);
                }
                ("queue", "submitted") if !offline => {
                    let (f, c) = detail.split_once(' ').context("invalid audit submission")?;
                    let c = c.parse::<u64>()?;
                    self.pipelines.entry(key.into()).or_default().submitted += c;
                    submissions.push((f.parse::<u64>()?, key.to_owned(), c));
                }
                ("offline-graphics" | "offline-compute" | "runtime" | "command", _)
                    if key != "-" =>
                {
                    let p = self.pipelines.entry(key.into()).or_default();
                    match (offline, event) {
                        (true, "requested") => p.offline += 1,
                        (true, "created") => p.offline_created += 1,
                        (true, "rejected") => p.offline_failed += 1,
                        (false, "requested") => p.requested += 1,
                        (false, "created") => p.created += 1,
                        (false, "rejected") => p.rejected += 1,
                        (false, "skipped") => p.skipped += 1,
                        (false, "pending") => p.pending += 1,
                        (false, "recorded") => p.recorded += 1,
                        (false, "missing") => p.missing += 1,
                        _ => {}
                    }
                    if event == "requested" {
                        let stages = detail.split_once(' ').map_or("", |d| d.1);
                        for shader in stages
                            .split(',')
                            .filter_map(|s| s.split_once('=').map(|d| d.1))
                        {
                            ensure!(
                                shader.len() == 64 && shader.bytes().all(|b| b.is_ascii_hexdigit()),
                                "invalid shader identity"
                            );
                            if !offline {
                                self.runtime_shaders.insert(shader.to_owned());
                            }
                        }
                        p.stages = stages.to_string();
                    }
                }
                _ => {}
            }
        }
        for (f, key, count) in submissions {
            if f <= finished {
                self.pipelines
                    .get_mut(&key)
                    .expect("submitted identity")
                    .finished += count;
            }
        }
        Ok(())
    }
}

pub fn run(offline: Vec<PathBuf>, runtime: Vec<PathBuf>, details: Option<PathBuf>) -> Result<()> {
    let mut report = Coverage::default();
    for (paths, is_offline) in [(&offline, true), (&runtime, false)] {
        for path in paths {
            report.read(path, is_offline)?;
        }
    }
    println!(
        "Scanned file observations {} ({} bytes).",
        report.first_party_files, report.scanned_bytes
    );
    println!(
        "Offline discovered shaders {}. Runtime requested shaders {}. Runtime shaders absent from the supplied offline inventory {}.",
        report.offline_shaders.len(),
        report.runtime_shaders.len(),
        report
            .runtime_shaders
            .difference(&report.offline_shaders)
            .count()
    );
    let known = report
        .translated_shaders
        .union(&report.rejected_shaders)
        .collect::<HashSet<_>>();
    println!(
        "Recorded translation outcomes: {} translated, {} rejected, {} discovered without an outcome. Additional unique unimplemented containers: {}.",
        report.translated_shaders.len(),
        report.rejected_shaders.len(),
        report
            .offline_shaders
            .iter()
            .filter(|s| !known.contains(s))
            .count(),
        report.other_containers.len()
    );
    let count = |pred: fn(&Pipeline) -> bool| report.pipelines.values().filter(|p| pred(p)).count();
    println!(
        "Distinct PSO keys. Offline requested {}, created {}, rejected {}. Runtime requested {}, created {}, rejected {}, skipped {}, pending at bind {}.",
        count(|p| p.offline > 0),
        count(|p| p.offline_created > 0),
        count(|p| p.offline_failed > 0),
        count(|p| p.requested > 0),
        count(|p| p.created > 0),
        count(|p| p.rejected > 0),
        count(|p| p.skipped > 0),
        count(|p| p.pending > 0)
    );
    println!(
        "Runtime-only requested keys {}. Requested keys with no host-creation outcome {}.",
        count(|p| p.requested > 0 && p.offline == 0),
        count(|p| p.requested > 0 && p.created + p.rejected + p.skipped == 0)
    );
    println!(
        "Keys with recorded commands {}, missing commands {}, submitted commands {}, and fence-confirmed commands {}.",
        count(|p| p.recorded > 0),
        count(|p| p.missing > 0),
        count(|p| p.submitted > 0),
        count(|p| p.finished > 0)
    );
    println!("Audit event counts");
    for ((src, event), n) in &report.counts {
        println!("{src}\t{event}\t{n}");
    }
    if let Some(path) = details {
        let mut f = BufWriter::new(File::create_new(path)?);
        writeln!(
            f,
            "pso\toffline_requests\toffline_created\toffline_rejected\truntime_requests\truntime_created\truntime_rejected\tskipped\tpending\trecorded\tmissing\tsubmitted\tfence_completed\tstages"
        )?;
        for (key, p) in &report.pipelines {
            writeln!(
                f,
                "{key}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                p.offline,
                p.offline_created,
                p.offline_failed,
                p.requested,
                p.created,
                p.rejected,
                p.skipped,
                p.pending,
                p.recorded,
                p.missing,
                p.submitted,
                p.finished,
                p.stages
            )?;
        }
        f.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const HEADER: &str = "sequence\tframe\tsource\tevent\tidentity\tdetail\n";

    #[test]
    fn completed_fences_only_cover_submissions_from_their_own_process() {
        let mut coverage = Coverage::default();
        let read = |c: &mut Coverage, text: &str| {
            c.read_journal(
                std::io::Cursor::new(format!("{HEADER}{text}")),
                Path::new("test"),
                false,
            )
        };
        read(&mut coverage, "1\t0\tqueue\tsubmitted\tfirst\t1 3\n2\t0\tqueue\tsubmitted\tlate\t3 9\n3\t0\tqueue\tcompleted_through\t-\t2\n").unwrap();
        read(&mut coverage, "1\t0\tqueue\tcompleted_through\t-\t10\n").unwrap();
        assert_eq!(coverage.pipelines["first"].finished, 3);
        assert_eq!(coverage.pipelines["late"].submitted, 9);
        assert_eq!(coverage.pipelines["late"].finished, 0);
        assert!(read(&mut coverage, "2\t0\tqueue\tcompleted_through\t-\t4\n").is_err());
        assert!(
            read(
                &mut coverage,
                "1\t0\tqueue\tcompleted_through\t-\t18446744073709551615\n"
            )
            .is_err()
        );
    }
}
