use rmpd_core::error::{Result, RmpdError};
use rmpd_player::SymphoniaDecoder;
use std::path::Path;
use std::sync::Mutex;

/// Maximum duration to fingerprint (120 seconds recommended by Chromaprint)
const MAX_FINGERPRINT_DURATION_SECS: u64 = 120;

fn to_i32_u32(value: u32, field: &str) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        RmpdError::Library(format!(
            "{field} value {value} exceeds supported chromaprint i32 range"
        ))
    })
}

fn to_i32_usize(value: usize, field: &str) -> Result<i32> {
    i32::try_from(value).map_err(|_| {
        RmpdError::Library(format!(
            "{field} value {value} exceeds supported chromaprint i32 range"
        ))
    })
}

fn max_samples(sample_rate: u32, channels: u8) -> Result<usize> {
    let per_second = u64::from(sample_rate)
        .checked_mul(u64::from(channels))
        .ok_or_else(|| RmpdError::Library("Fingerprint sample budget overflow".to_string()))?;
    let total = per_second
        .checked_mul(MAX_FINGERPRINT_DURATION_SECS)
        .ok_or_else(|| RmpdError::Library("Fingerprint sample budget overflow".to_string()))?;
    usize::try_from(total)
        .map_err(|_| RmpdError::Library("Fingerprint sample budget overflow".to_string()))
}

fn feed_len_for_budget(total_samples: usize, max_samples: usize, samples_read: usize) -> usize {
    max_samples.saturating_sub(total_samples).min(samples_read)
}

/// Serializes Chromaprint context creation/destruction.
///
/// `chromaprint_new` / `chromaprint_free` are NOT safe to call concurrently
/// (context setup/teardown touches non-reentrant global state, e.g. FFT plan
/// allocation), which corrupts the heap ("double free or corruption") when two
/// `Fingerprinter`s are created/dropped on different threads — as happens with
/// parallel `getfingerprint` commands or parallel tests. Holding this lock only
/// around the alloc/free FFI calls keeps that lifecycle serialized while leaving
/// per-context feeding/finishing concurrent.
static CHROMAPRINT_LOCK: Mutex<()> = Mutex::new(());

/// Audio fingerprinter using Chromaprint library
pub struct Fingerprinter {
    ctx: *mut chromaprint_sys_next::ChromaprintContext,
}

impl Fingerprinter {
    /// Create a new fingerprinter instance
    pub fn new() -> Result<Self> {
        // Use CHROMAPRINT_ALGORITHM_DEFAULT (value = 1)
        // SAFETY: chromaprint_new is a safe FFI call that allocates and initializes a new
        // Chromaprint context. The returned pointer is either valid or null; we check for
        // null immediately after and return an error if allocation failed.
        let ctx = {
            let _guard = CHROMAPRINT_LOCK
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            unsafe { chromaprint_sys_next::chromaprint_new(1) }
        };

        if ctx.is_null() {
            return Err(RmpdError::Library(
                "Failed to create chromaprint context".to_string(),
            ));
        }

        Ok(Self { ctx })
    }

    /// Generate a fingerprint for an audio file
    ///
    /// Returns a base64-encoded fingerprint string compatible with AcoustID.
    /// Only processes the first 120 seconds of audio as recommended by Chromaprint.
    pub fn fingerprint_file(&mut self, path: &Path) -> Result<String> {
        // Open audio file with Symphonia decoder
        let mut decoder = SymphoniaDecoder::open(path)?;

        // Get runtime-confirmed format info (channels are unknown until first frame).
        let format_info = decoder.format_info()?;
        let sample_rate = format_info.sample_rate;
        let channels = format_info.channels;
        let sample_rate_i32 = to_i32_u32(sample_rate, "sample_rate")?;
        let channels_i32 = to_i32_u32(u32::from(channels), "channels")?;

        // Initialize chromaprint with audio format
        // SAFETY: self.ctx is guaranteed to be non-null (checked in new()) and valid for the
        // lifetime of self. chromaprint_start initializes the context with audio format parameters.
        // The sample_rate and channels are valid i32 values derived from the decoder.
        let result = unsafe {
            chromaprint_sys_next::chromaprint_start(self.ctx, sample_rate_i32, channels_i32)
        };

        if result == 0 {
            return Err(RmpdError::Library(
                "Failed to initialize chromaprint".to_string(),
            ));
        }

        // Calculate maximum samples to process (120 seconds)
        let max_samples = max_samples(sample_rate, channels)?;
        let mut total_samples = 0;

        // Buffer for reading audio data
        let buffer_size = 4096;
        let mut f32_buffer = vec![0.0f32; buffer_size];
        let mut i16_buffer = vec![0i16; buffer_size];

        // Read and feed audio data to chromaprint
        loop {
            if total_samples >= max_samples {
                break;
            }

            // Read samples from decoder
            let samples_read = match decoder.read(&mut f32_buffer) {
                Ok(n) => n,
                Err(e) => return Err(e),
            };

            if samples_read == 0 {
                break;
            }

            let to_feed = feed_len_for_budget(total_samples, max_samples, samples_read);
            if to_feed == 0 {
                break;
            }

            // Convert f32 samples to i16 for chromaprint
            // Clamp to prevent overflow
            for (i, &sample) in f32_buffer[..to_feed].iter().enumerate() {
                let clamped = sample.clamp(-1.0, 1.0);
                i16_buffer[i] = (clamped * 32767.0) as i16;
            }

            let to_feed_i32 = to_i32_usize(to_feed, "samples_read")?;

            // Feed samples to chromaprint
            // SAFETY: self.ctx is valid and non-null (checked in new()). i16_buffer.as_ptr()
            // is a valid pointer to samples_read i16 elements. The pointer remains valid for
            // the duration of the FFI call. samples_read is guaranteed to be <= buffer_size.
            let result = unsafe {
                chromaprint_sys_next::chromaprint_feed(
                    self.ctx,
                    i16_buffer.as_ptr(),
                    to_feed_i32,
                )
            };

            if result == 0 {
                return Err(RmpdError::Library(
                    "Failed to feed samples to chromaprint".to_string(),
                ));
            }

            total_samples += to_feed;
        }

        // Finalize fingerprint
        // SAFETY: self.ctx is valid and non-null (checked in new()). chromaprint_finish
        // finalizes the fingerprint computation and prepares it for retrieval.
        let result = unsafe { chromaprint_sys_next::chromaprint_finish(self.ctx) };

        if result == 0 {
            return Err(RmpdError::Library(
                "Failed to finalize fingerprint".to_string(),
            ));
        }

        // Use chromaprint_get_fingerprint_hash for a compact hash representation
        // or chromaprint_get_fingerprint for the compressed base64 string
        let mut fp_str: *mut std::os::raw::c_char = std::ptr::null_mut();

        // SAFETY: self.ctx is valid and non-null (checked in new()). &mut fp_str is a valid
        // mutable pointer to a C string pointer. chromaprint_get_fingerprint will write a
        // pointer to the fingerprint string into fp_str if successful.
        let result =
            unsafe { chromaprint_sys_next::chromaprint_get_fingerprint(self.ctx, &mut fp_str) };

        if result == 0 || fp_str.is_null() {
            return Err(RmpdError::Library("Failed to get fingerprint".to_string()));
        }

        // Convert C string to Rust String
        // SAFETY: fp_str is guaranteed to be non-null (checked above) and points to a
        // valid null-terminated C string allocated by Chromaprint. CStr::from_ptr is safe
        // because the pointer is valid and the string is null-terminated.
        let encoded = unsafe {
            let c_str = std::ffi::CStr::from_ptr(fp_str);
            c_str.to_string_lossy().into_owned()
        };

        // Free chromaprint memory
        // SAFETY: fp_str is a valid pointer to memory allocated by Chromaprint's
        // chromaprint_get_fingerprint. chromaprint_dealloc is the correct deallocation
        // function for this memory. We only deallocate once.
        unsafe {
            chromaprint_sys_next::chromaprint_dealloc(fp_str as *mut std::ffi::c_void);
        }

        Ok(encoded)
    }
}

impl Drop for Fingerprinter {
    fn drop(&mut self) {
        if !self.ctx.is_null() {
            // SAFETY: self.ctx is a valid pointer to a Chromaprint context allocated by
            // chromaprint_new. chromaprint_free is the correct deallocation function.
            // We only call it once per Fingerprinter instance, and the null check ensures
            // we don't double-free.
            let _guard = CHROMAPRINT_LOCK
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            unsafe {
                chromaprint_sys_next::chromaprint_free(self.ctx);
            }
            self.ctx = std::ptr::null_mut();
        }
    }
}

unsafe impl Send for Fingerprinter {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprinter_creation() {
        let fingerprinter = Fingerprinter::new();
        assert!(fingerprinter.is_ok());
    }

    #[test]
    fn test_fingerprint_nonexistent_file() {
        let mut fingerprinter = Fingerprinter::new().unwrap();
        let result = fingerprinter.fingerprint_file(Path::new("/nonexistent/file.mp3"));
        assert!(result.is_err());
    }

    #[test]
    fn test_feed_len_for_budget_clamps_to_remaining_samples() {
        assert_eq!(feed_len_for_budget(0, 100, 32), 32);
        assert_eq!(feed_len_for_budget(90, 100, 32), 10);
        assert_eq!(feed_len_for_budget(100, 100, 32), 0);
        assert_eq!(feed_len_for_budget(110, 100, 32), 0);
    }

    #[test]
    fn test_i32_conversions_validate_bounds() {
        assert_eq!(to_i32_u32(48_000, "sample_rate").unwrap(), 48_000);
        assert!(to_i32_u32(u32::MAX, "sample_rate").is_err());

        assert_eq!(to_i32_usize(4096, "samples_read").unwrap(), 4096);
        assert!(to_i32_usize((i32::MAX as usize) + 1, "samples_read").is_err());
    }

    #[test]
    fn test_max_samples_computation() {
        let total = max_samples(48_000, 2).unwrap();
        assert_eq!(total, 48_000 * 2 * MAX_FINGERPRINT_DURATION_SECS as usize);
    }
}
