//! The ONE per-file content digest in benchd: one streamed sha256 per file, one bounded
//! worker pool, and one process-local memo so a file's bytes are read and hashed ONCE per
//! `benchd` process no matter how many consumers need that file's digest.
//!
//! ## Why this module exists
//!
//! A ranked `measure-job` used to read every source byte of both workspaces TWICE:
//!
//! * `editable_divergence::hash_tree` read the candidate tree and the baseline tree with a
//!   SERIAL whole-file `fs::read` per entry, to build the write-outside-editablePaths diff;
//! * `iterate::dir_digest` then read both trees again — in parallel, streamed — to seal
//!   `candidate_workspace_sha256` / the baseline workspace digest into the integrity anchor.
//!
//! Neither read could be deleted: the two walks visit DIFFERENT node sets (the divergence
//! gate holds `.git`/`.build`/`.build-worker`/`weights` out and never descends a symlinked
//! directory; the workspace seal covers everything and follows symlinks the way the Swift
//! reference does), so one walk cannot serve both without changing what each consumer sees.
//! What they DO share is the expensive part: for a file both walks reach, `sha256(bytes)` is
//! the same number computed twice.
//!
//! So the walks stay separate — each keeps its exact semantics — and the HASHING is unified
//! here. The second walk to reach a file gets its digest from the memo and never opens it.
//!
//! ## The memo key, and what it assumes
//!
//! `(dev, ino, mtime_ns, size)`. A hit means "the same inode, the same length, and the same
//! last-modified nanosecond" — the standard stat-cache identity. A file rewritten between the
//! two walks gets a new `mtime_ns` (APFS and ext4 both stamp nanoseconds) and therefore MISSES
//! and is re-hashed, which is the behaviour the gate needs. The memo is an OPTIMISATION ONLY:
//! it is process-local, starts empty, and every miss is a full re-hash, so a run with the memo
//! disabled produces byte-identical digests to one with it.
//!
//! ## The sidecar (the WEIGHTS tree only)
//!
//! The memo above dies with the process, and the ~105 GiB weights tree is hashed by TWO
//! processes minutes apart: `benchd weights-digest` produces the window's value, and a scored
//! `iterate` hashes the same immutable tree again for itself (it must — `--weights-digest` is
//! refused outside `--capture-baseline`, so a scored seal never takes a value on trust). The
//! sidecar carries the SAME `(dev, ino, mtime_ns, size) -> sha256` entries across that gap, on
//! disk, under the user cache directory — never beside the weights, which may be read-only.
//!
//! It is scoped to the weights digest by an RAII guard ([`weights_cache_session`]) so a
//! workspace walk never writes source-tree entries into it, and it changes NOTHING about how a
//! directory digest is assembled: the per-file digests are the same numbers, folded in the same
//! sorted order, so the result is byte-identical. Every failure mode degrades to a rehash —
//! no cache directory, an absent file, a line that does not parse, a line whose integrity tag
//! does not match, a key that does not match the file on disk. See [`CACHE_FORMAT`].
//!
//! ## What is NOT unified here
//!
//! `byte_budget`'s walks (`walk_dir`, `sum_regular_files`) are STAT-ONLY — they never open a
//! file — and they are scoped to the `editablePaths` subtrees, not the whole workspace. They
//! are already at their floor, they differ from each other in failure policy (fail-closed with
//! a deterministic message vs. best-effort) and in short-circuit behaviour, and folding them
//! into a content-hashing walk would make a cheap check expensive. They are left alone
//! deliberately.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex};

use sha2::{Digest, Sha256};

/// Worker cap for the parallel per-file hash. The weight tree is a handful of multi-GB
/// safetensors on one store; a few readers saturate it, and each worker holds its own
/// 8 MiB read buffer.
const MAX_WORKERS: usize = 8;

/// Chunk size for the streamed read — the Swift `fileDigest` chunk size, so a multi-GB
/// safetensors file is never buffered whole.
const CHUNK_BYTES: usize = 8 * 1024 * 1024;

/// The stat identity a memoised digest is keyed by. See the module doc for what a hit
/// assumes; `size` is carried both as part of the identity and as the value the caller
/// needs, so a memo hit does not have to re-open the file to learn its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileKey {
    dev: u64,
    ino: u64,
    mtime_s: i64,
    mtime_ns: i64,
    size: u64,
}

impl FileKey {
    /// The key for an already-stat'ed file.
    pub(crate) fn from_metadata(md: &fs::Metadata) -> FileKey {
        FileKey {
            dev: md.dev(),
            ino: md.ino(),
            mtime_s: md.mtime(),
            mtime_ns: md.mtime_nsec(),
            size: md.size(),
        }
    }

    /// The file's length as of the stat this key was taken from.
    pub(crate) fn size(&self) -> u64 {
        self.size
    }
}

/// The process-local memo. Bounded in practice by the number of distinct files one benchd
/// process hashes (~40 bytes of key + 32 of digest per entry).
static MEMO: LazyLock<Mutex<HashMap<FileKey, [u8; 32]>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn memo_get(key: &FileKey) -> Option<[u8; 32]> {
    // A poisoned memo would mean a panic inside the map itself; treat it as a miss rather
    // than propagating — the memo is an optimisation and a miss is always correct.
    MEMO.lock().ok().and_then(|m| m.get(key).copied())
}

fn memo_put(key: FileKey, digest: [u8; 32]) {
    if let Ok(mut m) = MEMO.lock() {
        m.insert(key, digest);
    }
}

/// Record a FRESHLY COMPUTED digest: into the process memo, and into the sidecar when a
/// [`WeightsCacheSession`] is open. Entries LOADED from the sidecar go straight to [`memo_put`]
/// so they are not written back.
fn record(key: FileKey, digest: [u8; 32]) {
    memo_put(key, digest);
    sink_append(&key, &digest);
}

/// Drop every memoised digest. TEST-ONLY: proves that an EMPTY memo produces the same
/// digests a warm one does, which is the "optimisation only" property stated above.
#[cfg(test)]
pub(crate) fn clear_memo_for_test() {
    if let Ok(mut m) = MEMO.lock() {
        m.clear();
    }
}

/// The worker count the parallel hash uses for `file_count` files:
/// `min(available_parallelism, MAX_WORKERS, file_count)`, never below 1. Named once so a
/// pool and a caller's stderr timing line can never report different numbers.
pub(crate) fn workers_for(file_count: usize) -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(MAX_WORKERS)
        .clamp(1, file_count.max(1))
}

/// `sha256` of one file's bytes plus its length, streamed in 8 MiB chunks and served from
/// the process memo when the same inode at the same mtime and length was hashed already.
///
/// The read deliberately BYPASSES the page cache. This hasher streams the FULL weight tree
/// (~105 GB) right before the worker loads the ~90 GiB model; with buffered reads macOS fills
/// the unified file cache with reclaimable weight pages, which then get evicted (churn +
/// memory pressure) exactly as the load starts. The hashed bytes — and therefore the digest —
/// are identical either way; only the OS cache behaviour changes. (Mirrors the engine's
/// `DenseTensorStore` F_NOCACHE loader.)
pub(crate) fn sha256_file(path: &Path) -> io::Result<([u8; 32], u64)> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    // One stat, from the OPEN handle: the key is taken from the very file the bytes below
    // come from, so a path swapped between the stat and the read cannot produce a key that
    // names a different inode than the digest covers.
    let key = file.metadata().ok().map(|md| FileKey::from_metadata(&md));
    if let Some(key) = key {
        if let Some(digest) = memo_get(&key) {
            return Ok((digest, key.size()));
        }
    }

    #[cfg(target_os = "macos")]
    {
        use std::os::unix::io::AsRawFd;
        // SAFETY: `file` owns a valid open fd for the whole call; F_NOCACHE only tells the
        // kernel to bypass the buffer cache for this fd's reads. Return value is advisory.
        unsafe {
            libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1);
        }
    }
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    // Linux has no F_NOCACHE; drop the pages we just streamed so the imminent model load is
    // not competing with reclaimable weight-hash pages. Advisory, digest-neutral.
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::io::AsRawFd;
        // SAFETY: `file` owns a valid open fd; POSIX_FADV_DONTNEED is a page-cache hint only.
        unsafe {
            libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
        }
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    // Memoise only when the length the stat promised is the length actually read: a file
    // growing under the walk is not an identity the next consumer may reuse.
    if let Some(key) = key {
        if key.size() == size {
            record(key, out);
        }
    }
    Ok((out, size))
}

/// The FIRST per-file failure a parallel hash hit, with the INDEX of the path that failed so
/// a caller whose refusal text names the file can still name it. Converts into a bare
/// `io::Error` for callers that only propagate.
#[derive(Debug)]
pub(crate) struct FileHashError {
    pub(crate) index: usize,
    pub(crate) error: io::Error,
}

impl From<FileHashError> for io::Error {
    fn from(e: FileHashError) -> io::Error {
        e.error
    }
}

/// Hash every path in `paths` on a bounded pool of scoped threads pulling from one shared
/// index, returning `(index, sha256, byte count)` sorted ascending by index so the caller
/// folds them in exactly the order a serial loop would have.
///
/// On I/O failure the FIRST error in index order is returned and the remaining workers stop
/// early, matching a serial loop's `?` on the first failing file.
///
/// `force_workers` pins the pool size (tests pin it); production passes `None` for
/// [`workers_for`].
pub(crate) fn sha256_files_parallel(
    paths: &[&Path],
    force_workers: Option<usize>,
) -> Result<Vec<(usize, [u8; 32], u64)>, FileHashError> {
    let workers = match force_workers {
        Some(forced) => forced.clamp(1, paths.len().max(1)),
        None => workers_for(paths.len()),
    };

    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let outputs = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut done: Vec<(usize, [u8; 32], u64)> = Vec::new();
                    let mut failed: Option<(usize, io::Error)> = None;
                    while !stop.load(Ordering::Relaxed) {
                        let idx = next.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = paths.get(idx) else {
                            break;
                        };
                        match sha256_file(path) {
                            Ok((file_sha, size)) => done.push((idx, file_sha, size)),
                            Err(err) => {
                                stop.store(true, Ordering::Relaxed);
                                failed = Some((idx, err));
                                break;
                            }
                        }
                    }
                    (done, failed)
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| match handle.join() {
                Ok(out) => out,
                // A hashing worker can only panic on a bug; re-raise it on this thread
                // rather than sealing a digest over a partial file set.
                Err(payload) => std::panic::resume_unwind(payload),
            })
            .collect::<Vec<_>>()
    });

    let mut hashed: Vec<(usize, [u8; 32], u64)> = Vec::with_capacity(paths.len());
    let mut first_err: Option<(usize, io::Error)> = None;
    for (done, failed) in outputs {
        hashed.extend(done);
        if let Some((idx, err)) = failed {
            match &first_err {
                Some((seen, _)) if *seen <= idx => {}
                _ => first_err = Some((idx, err)),
            }
        }
    }
    if let Some((index, error)) = first_err {
        return Err(FileHashError { index, error });
    }
    hashed.sort_by_key(|(idx, _, _)| *idx);
    Ok(hashed)
}

/// The sidecar's on-disk format, one line per file, newline-terminated:
///
/// ```text
/// v1 <dev> <ino> <mtime_s> <mtime_ns> <size> <sha256hex> <tag>
/// ```
///
/// `<tag>` is the first 16 hex characters of `sha256` over the line's payload — everything
/// before the final space. It is not a security property (the file is ours, in the user's own
/// cache directory); it is what makes "a corrupted cache produces the same digest" TRUE rather
/// than hoped: without it, a single flipped character inside `<sha256hex>` would still parse and
/// would hand back a wrong digest. With it, any damaged line fails its tag, is dropped, and the
/// file it named is simply rehashed.
///
/// Lines are appended with `O_APPEND` and are far under `PIPE_BUF`, so two benchd processes
/// writing at once interleave whole lines rather than fragments.
const CACHE_FORMAT: &str = "v1";

/// Above this, the sidecar is DELETED and started over on the next load. It is a cache of stat
/// identities, so entries go stale as trees are rebuilt and nothing ever removes them one by
/// one; ~8 MiB is about 50k entries, far more than any tree benchd hashes.
const CACHE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// The append sink, `Some` only while a [`weights_cache_session`] guard is alive.
static SINK: Mutex<Option<fs::File>> = Mutex::new(None);

/// The user cache directory this sidecar lives under: `$XDG_CACHE_HOME` when set to an absolute
/// path, else the platform default (`~/Library/Caches` on macOS, `~/.cache` elsewhere). `None`
/// when neither resolves, which simply means no sidecar this run.
fn cache_root() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        let p = PathBuf::from(xdg);
        if p.is_absolute() {
            return Some(p);
        }
    }
    let home = PathBuf::from(std::env::var_os("HOME")?);
    if !home.is_absolute() {
        return None;
    }
    if cfg!(target_os = "macos") {
        Some(home.join("Library/Caches"))
    } else {
        Some(home.join(".cache"))
    }
}

/// The sidecar path: `<cache root>/benchd/weights-file-digests`.
fn cache_path() -> Option<PathBuf> {
    Some(cache_root()?.join("benchd").join("weights-file-digests"))
}

/// The first 16 hex characters of `sha256(payload)` — the per-line integrity tag.
fn line_tag(payload: &str) -> String {
    let mut h = Sha256::new();
    h.update(payload.as_bytes());
    bench_core::hash::hex_lower(&h.finalize())[..16].to_string()
}

/// One sidecar line for `key` -> `digest`, newline included.
fn cache_line(key: &FileKey, digest: &[u8; 32]) -> String {
    let payload = format!(
        "{CACHE_FORMAT} {} {} {} {} {} {}",
        key.dev,
        key.ino,
        key.mtime_s,
        key.mtime_ns,
        key.size,
        bench_core::hash::hex_lower(digest)
    );
    let tag = line_tag(&payload);
    format!("{payload} {tag}\n")
}

/// Parse one sidecar line. `None` for ANY line this version does not recognise as intact: wrong
/// field count, wrong format tag, a field that does not parse, a malformed digest, or a tag that
/// does not match the payload.
fn parse_cache_line(line: &str) -> Option<(FileKey, [u8; 32])> {
    let (payload, tag) = line.trim_end().rsplit_once(' ')?;
    if line_tag(payload) != tag {
        return None;
    }
    let mut f = payload.split(' ');
    if f.next()? != CACHE_FORMAT {
        return None;
    }
    let key = FileKey {
        dev: f.next()?.parse().ok()?,
        ino: f.next()?.parse().ok()?,
        mtime_s: f.next()?.parse().ok()?,
        mtime_ns: f.next()?.parse().ok()?,
        size: f.next()?.parse().ok()?,
    };
    let hex = f.next()?;
    if f.next().is_some() || hex.len() != 64 {
        return None;
    }
    let mut digest = [0u8; 32];
    for (i, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some((key, digest))
}

/// Every intact entry in the sidecar at `path`. A missing file, an unreadable one, or a file
/// of garbage all yield nothing — the caller then hashes for itself.
fn read_cache_file(path: &Path) -> Vec<(FileKey, [u8; 32])> {
    let Ok(body) = fs::read_to_string(path) else {
        return Vec::new();
    };
    body.lines().filter_map(parse_cache_line).collect()
}

/// Scopes the sidecar to ONE digest. While the returned guard lives, the loaded entries are in
/// the process memo and every NEWLY computed digest is appended to the sidecar; on drop the sink
/// closes and later walks in the same process are memo-only again.
///
/// Every step is best-effort: no cache directory, an unwritable one, an over-large sidecar (it
/// is deleted and restarted) — all of them just mean this run hashes for itself.
#[must_use = "the sidecar is open only while the guard lives"]
pub(crate) struct WeightsCacheSession;

impl Drop for WeightsCacheSession {
    fn drop(&mut self) {
        if let Ok(mut sink) = SINK.lock() {
            *sink = None;
        }
    }
}

/// Open a [`WeightsCacheSession`] over the default sidecar path.
pub(crate) fn weights_cache_session() -> WeightsCacheSession {
    match cache_path() {
        Some(path) => weights_cache_session_at(&path),
        None => WeightsCacheSession,
    }
}

/// [`weights_cache_session`] against an explicit path, so the format and the transparency
/// property are testable without touching the real user cache directory.
fn weights_cache_session_at(path: &Path) -> WeightsCacheSession {
    if fs::metadata(path).map(|m| m.len()).unwrap_or(0) > CACHE_MAX_BYTES {
        let _ = fs::remove_file(path);
    }
    for (key, digest) in read_cache_file(path) {
        memo_put(key, digest);
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        if let Ok(mut sink) = SINK.lock() {
            *sink = Some(file);
        }
    }
    WeightsCacheSession
}

/// Append one entry to the open sidecar, if there is one. One `write_all` of one short line, so
/// an `O_APPEND` fd never interleaves a partial record with another process's.
fn sink_append(key: &FileKey, digest: &[u8; 32]) {
    use std::io::Write;
    if let Ok(mut sink) = SINK.lock() {
        if let Some(file) = sink.as_mut() {
            let _ = file.write_all(cache_line(key, digest).as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testtmp::tmp;

    /// `SINK` is process-global, so the two tests that OPEN a session must not overlap — one
    /// would otherwise close the other's sink mid-test. Nothing else in the suite opens one.
    static SIDECAR: Mutex<()> = Mutex::new(());

    fn reference_sha256(bytes: &[u8]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(bytes);
        h.finalize().into()
    }

    /// The streamed hash equals a one-shot hash of the same bytes, across the 8 MiB chunk
    /// boundary (which is where a chunked reader with an off-by-one would first show).
    #[test]
    fn streamed_digest_matches_a_one_shot_hash_across_the_chunk_boundary() {
        let d = tmp("chunk");
        let body: Vec<u8> = (0..(CHUNK_BYTES + 12345))
            .map(|i| (i % 251) as u8)
            .collect();
        let p = d.join("big.bin");
        fs::write(&p, &body).unwrap();
        let (got, size) = sha256_file(&p).unwrap();
        assert_eq!(size, body.len() as u64);
        assert_eq!(got, reference_sha256(&body));
        let _ = fs::remove_dir_all(&d);
    }

    /// The memo is an OPTIMISATION ONLY: a cold memo and a warm one produce the same digest,
    /// and REWRITING the file invalidates the entry (new mtime/length) so the next read sees
    /// the new bytes rather than the memoised digest of the old ones.
    #[test]
    fn memo_is_transparent_and_invalidated_by_a_rewrite() {
        let d = tmp("memo");
        let p = d.join("a.txt");
        fs::write(&p, b"first").unwrap();

        clear_memo_for_test();
        let cold = sha256_file(&p).unwrap();
        let warm = sha256_file(&p).unwrap(); // served from the memo
        assert_eq!(cold, warm);
        assert_eq!(cold.0, reference_sha256(b"first"));

        clear_memo_for_test();
        let cold_again = sha256_file(&p).unwrap();
        assert_eq!(cold_again, warm, "an empty memo must give the same answer");

        // Rewrite with DIFFERENT content and a different length, then prove the memo did not
        // serve the stale digest.
        fs::write(&p, b"second-and-longer").unwrap();
        let after = sha256_file(&p).unwrap();
        assert_eq!(after.0, reference_sha256(b"second-and-longer"));
        let _ = fs::remove_dir_all(&d);
    }

    /// The pool returns index-ordered results regardless of worker count, and every worker
    /// count produces the same digests.
    #[test]
    fn parallel_hash_is_index_ordered_and_worker_count_independent() {
        let d = tmp("pool");
        let mut paths = Vec::new();
        for i in 0..17 {
            let p = d.join(format!("f{i:02}.bin"));
            fs::write(&p, format!("body-{i}").as_bytes()).unwrap();
            paths.push(p);
        }
        let refs: Vec<&Path> = paths.iter().map(std::path::PathBuf::as_path).collect();
        let serial = sha256_files_parallel(&refs, Some(1)).unwrap();
        assert_eq!(
            serial.iter().map(|(i, _, _)| *i).collect::<Vec<_>>(),
            (0..paths.len()).collect::<Vec<_>>()
        );
        for workers in [2usize, 4, 8] {
            clear_memo_for_test();
            assert_eq!(sha256_files_parallel(&refs, Some(workers)).unwrap(), serial);
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// The sidecar round-trips a key and its digest.
    #[test]
    fn a_cache_line_round_trips() {
        let key = FileKey {
            dev: 16777232,
            ino: 12345678901234,
            mtime_s: 1789000000,
            mtime_ns: 123456789,
            size: 105_000_000_000,
        };
        let digest = reference_sha256(b"weights-shard");
        let line = cache_line(&key, &digest);
        assert!(line.ends_with('\n'));
        assert_eq!(parse_cache_line(&line), Some((key, digest)));
    }

    /// EVERY single-character corruption of an intact line is DROPPED, not misread. This is the
    /// property "a corrupted cache produces the same digest" rests on: a damaged line yields no
    /// entry, the file it named misses, and it is rehashed.
    #[test]
    fn any_single_character_corruption_drops_the_line() {
        let key = FileKey {
            dev: 1,
            ino: 2,
            mtime_s: 3,
            mtime_ns: 4,
            size: 5,
        };
        let line = cache_line(&key, &reference_sha256(b"x"));
        let intact: Vec<char> = line.trim_end().chars().collect();
        for i in 0..intact.len() {
            let mut damaged = intact.clone();
            damaged[i] = if damaged[i] == 'a' { 'b' } else { 'a' };
            let damaged: String = damaged.into_iter().collect();
            assert_eq!(
                parse_cache_line(&damaged),
                None,
                "a line damaged at {i} must be dropped: {damaged}"
            );
        }
        // Garbage that is not a line at all is dropped too.
        assert_eq!(parse_cache_line(""), None);
        assert_eq!(parse_cache_line("v1 not a real line"), None);
    }

    /// END TO END over a real tree: the sidecar does not change the directory digest — warm,
    /// cold, or corrupted — and TOUCHING a file (same bytes, new mtime) rehashes it to the same
    /// value rather than serving the stale key.
    #[test]
    fn the_sidecar_never_changes_the_directory_digest() {
        let _serial = SIDECAR.lock().unwrap_or_else(|e| e.into_inner());
        let d = tmp("sidecar");
        let tree = d.join("tree");
        fs::create_dir_all(tree.join("sub")).unwrap();
        fs::write(tree.join("a.bin"), b"alpha").unwrap();
        fs::write(tree.join("sub/b.bin"), b"beta").unwrap();
        let cache = d.join("cache/weights-file-digests");

        // Cold: no sidecar at all.
        clear_memo_for_test();
        let cold = crate::iterate::dir_digest(&tree).unwrap();

        // First session writes the sidecar.
        {
            let _s = weights_cache_session_at(&cache);
            clear_memo_for_test();
            assert_eq!(crate::iterate::dir_digest(&tree).unwrap(), cold);
        }
        assert!(
            !read_cache_file(&cache).is_empty(),
            "the session must have written entries"
        );

        // Second session reads it back with an EMPTY process memo — the served path.
        {
            clear_memo_for_test();
            let _s = weights_cache_session_at(&cache);
            assert_eq!(crate::iterate::dir_digest(&tree).unwrap(), cold);
        }

        // TOUCH one file: same bytes, new mtime. Its key misses, it is rehashed, and the
        // directory digest is unchanged.
        fs::write(tree.join("a.bin"), b"alpha").unwrap();
        {
            clear_memo_for_test();
            let _s = weights_cache_session_at(&cache);
            assert_eq!(crate::iterate::dir_digest(&tree).unwrap(), cold);
        }

        // CORRUPT the sidecar outright.
        fs::write(&cache, b"v1 garbage\nnot even a line\n\x00\x01\x02\n").unwrap();
        {
            clear_memo_for_test();
            let _s = weights_cache_session_at(&cache);
            assert_eq!(crate::iterate::dir_digest(&tree).unwrap(), cold);
        }

        // An ABSENT sidecar, and an unwritable cache root, are both just misses.
        let _ = fs::remove_file(&cache);
        {
            clear_memo_for_test();
            let _s = weights_cache_session_at(&cache);
            assert_eq!(crate::iterate::dir_digest(&tree).unwrap(), cold);
        }
        clear_memo_for_test();
        let _ = fs::remove_dir_all(&d);
    }

    /// An over-large sidecar is discarded rather than loaded — it is a cache of stat identities
    /// and nothing prunes it entry by entry.
    #[test]
    fn an_oversized_sidecar_is_discarded() {
        let _serial = SIDECAR.lock().unwrap_or_else(|e| e.into_inner());
        let d = tmp("oversized");
        let cache = d.join("weights-file-digests");
        fs::write(&cache, vec![b'x'; (CACHE_MAX_BYTES + 1) as usize]).unwrap();
        drop(weights_cache_session_at(&cache));
        assert!(
            fs::metadata(&cache).map(|m| m.len()).unwrap_or(0) < CACHE_MAX_BYTES,
            "the oversized sidecar must have been restarted"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// A per-file I/O error surfaces as the FIRST failing index, as a serial `?` would.
    #[test]
    fn parallel_hash_surfaces_a_per_file_io_error() {
        let d = tmp("ioerr");
        let ok = d.join("ok.bin");
        fs::write(&ok, b"x").unwrap();
        let missing = d.join("missing.bin");
        let err = sha256_files_parallel(&[ok.as_path(), missing.as_path()], Some(2)).unwrap_err();
        assert_eq!(err.index, 1, "the failing path's index must be reported");
        assert_eq!(err.error.kind(), io::ErrorKind::NotFound);
        let _ = fs::remove_dir_all(&d);
    }
}
