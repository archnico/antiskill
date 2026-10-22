use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn unique_temp_path(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("orologio precedente a UNIX_EPOCH")
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}-{}-{nonce}-{counter}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::unique_temp_path;
    use std::collections::BTreeSet;

    #[test]
    fn rapid_calls_are_unique() {
        let paths = (0..10_000)
            .map(|_| unique_temp_path("antiskill-support-test"))
            .collect::<BTreeSet<_>>();
        assert_eq!(paths.len(), 10_000);
    }

    #[test]
    fn parallel_calls_are_unique() {
        let handles = (0..16)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..1_000)
                        .map(|_| unique_temp_path("antiskill-support-parallel"))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let paths = handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(paths.len(), 16_000);
    }
}
