use anyhow::{Context, Result};

/// Bytes fetched from the start of a remote GGUF file to scan its metadata.
/// Architecture-specific scalar keys (including `nextn_predict_layers`) are
/// written before the large tokenizer vocab arrays by llama.cpp's conversion
/// scripts, so this comfortably covers them without downloading the whole file.
const RANGE_BYTES: u64 = 8 * 1024 * 1024;

struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        if end > self.buf.len() {
            return None;
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Some(slice)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let len = self.u64()? as usize;
        let bytes = self.take(len)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    /// Skip a metadata value of the given GGUF value-type tag (see
    /// ggml-org/ggml's docs/gguf.md for the type enum and array layout).
    fn skip_value(&mut self, value_type: u32) -> Option<()> {
        match value_type {
            0 | 1 | 7 => {
                self.take(1)?;
            }
            2 | 3 => {
                self.take(2)?;
            }
            4..=6 => {
                self.take(4)?;
            }
            10..=12 => {
                self.take(8)?;
            }
            8 => {
                self.string()?;
            }
            9 => {
                let elem_type = self.u32()?;
                let len = self.u64()?;
                for _ in 0..len {
                    self.skip_value(elem_type)?;
                }
            }
            _ => return None,
        }
        Some(())
    }
}

/// Scan the first few MiB of a remote GGUF file's metadata for a key ending in
/// `.nextn_predict_layers` — the marker llama.cpp uses to detect models with
/// Multi-Token Prediction (MTP) heads baked into the main checkpoint (e.g.
/// DeepSeek-V3/R1, GLM-4.5+).
///
/// Only fetches a byte range, never the whole file. Best-effort: returns
/// `Ok(false)` rather than an error if the range is truncated mid-value or the
/// key just isn't present — this is a convenience heuristic, not worth failing
/// a model registration over.
pub async fn has_builtin_mtp(url: &str) -> Result<bool> {
    let client = reqwest::Client::builder()
        .user_agent("yllama/0.1")
        .build()?;
    let resp = client
        .get(url)
        .header("Range", format!("bytes=0-{}", RANGE_BYTES - 1))
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;

    if !resp.status().is_success() {
        return Ok(false);
    }
    let bytes = resp.bytes().await?;
    Ok(scan_for_nextn_key(&bytes))
}

/// Same MTP probe as [`has_builtin_mtp`], against a GGUF already on disk.
/// Reads at most the first few MiB, so it costs nothing on a 30 GB model.
pub fn has_builtin_mtp_local(path: &std::path::Path) -> Result<bool> {
    use std::io::Read;

    let file = std::fs::File::open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    let mut buf = Vec::new();
    file.take(RANGE_BYTES)
        .read_to_end(&mut buf)
        .with_context(|| format!("reading {}", path.display()))?;
    Ok(scan_for_nextn_key(&buf))
}

fn scan_for_nextn_key(buf: &[u8]) -> bool {
    let mut c = Cursor::new(buf);

    let Some(magic) = c.u32() else { return false };
    if magic != 0x4655_4747 {
        return false;
    }
    let Some(_version) = c.u32() else {
        return false;
    };
    let Some(_tensor_count) = c.u64() else {
        return false;
    };
    let Some(kv_count) = c.u64() else {
        return false;
    };

    for _ in 0..kv_count {
        let Some(key) = c.string() else { return false };
        let Some(value_type) = c.u32() else {
            return false;
        };
        if key.ends_with(".nextn_predict_layers") {
            return true;
        }
        if c.skip_value(value_type).is_none() {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_string(buf: &mut Vec<u8>, s: &str) {
        buf.extend_from_slice(&(s.len() as u64).to_le_bytes());
        buf.extend_from_slice(s.as_bytes());
    }

    fn header(buf: &mut Vec<u8>, kv_count: u64) {
        buf.extend_from_slice(&0x4655_4747u32.to_le_bytes());
        buf.extend_from_slice(&3u32.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes());
        buf.extend_from_slice(&kv_count.to_le_bytes());
    }

    #[test]
    fn detects_nextn_key() {
        let mut buf = Vec::new();
        header(&mut buf, 2);

        write_string(&mut buf, "general.architecture");
        buf.extend_from_slice(&8u32.to_le_bytes());
        write_string(&mut buf, "deepseek2");

        write_string(&mut buf, "deepseek2.nextn_predict_layers");
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());

        assert!(scan_for_nextn_key(&buf));
    }

    #[test]
    fn no_nextn_key_returns_false() {
        let mut buf = Vec::new();
        header(&mut buf, 1);

        write_string(&mut buf, "general.architecture");
        buf.extend_from_slice(&8u32.to_le_bytes());
        write_string(&mut buf, "qwen35moe");

        assert!(!scan_for_nextn_key(&buf));
    }

    #[test]
    fn skips_array_values_before_finding_key() {
        let mut buf = Vec::new();
        header(&mut buf, 2);

        // tokenizer.ggml.tokens = ["a", "b", "c"] (ARRAY of STRING)
        write_string(&mut buf, "tokenizer.ggml.tokens");
        buf.extend_from_slice(&9u32.to_le_bytes());
        buf.extend_from_slice(&8u32.to_le_bytes());
        buf.extend_from_slice(&3u64.to_le_bytes());
        write_string(&mut buf, "a");
        write_string(&mut buf, "b");
        write_string(&mut buf, "c");

        write_string(&mut buf, "glm4moe.nextn_predict_layers");
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());

        assert!(scan_for_nextn_key(&buf));
    }

    #[test]
    fn truncated_buffer_returns_false_not_panic() {
        let buf = vec![0x47, 0x47, 0x55, 0x46, 3, 0];
        assert!(!scan_for_nextn_key(&buf));
    }

    #[test]
    fn wrong_magic_returns_false() {
        let buf = vec![0u8; 32];
        assert!(!scan_for_nextn_key(&buf));
    }
}
