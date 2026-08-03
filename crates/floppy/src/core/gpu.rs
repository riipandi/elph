//! GPU detection and configuration for embed_anything.

use std::sync::OnceLock;

/// Detected GPU backend availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuBackend {
    /// No GPU available or disabled.
    None,
    /// Apple Metal (Apple Silicon, macOS).
    Metal,
    /// NVIDIA CUDA (Linux/Windows).
    Cuda,
}

/// GPU configuration for embedder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GpuConfig {
    /// Whether GPU acceleration is enabled by user preference.
    pub enabled: bool,
    /// Detected available backend.
    pub available_backend: GpuBackend,
}

impl GpuConfig {
    /// Detect GPU availability and return recommended config.
    pub fn detect() -> Self {
        let available_backend = detect_gpu_backend();
        let enabled = matches!(available_backend, GpuBackend::Metal | GpuBackend::Cuda);
        Self {
            enabled,
            available_backend,
        }
    }

    /// Create config with explicit user preference.
    pub fn with_preference(user_enabled: bool) -> Self {
        let available_backend = detect_gpu_backend();
        let enabled = user_enabled && matches!(available_backend, GpuBackend::Metal | GpuBackend::Cuda);
        Self {
            enabled,
            available_backend,
        }
    }

    /// Get the Candle device string for embed_anything.
    /// Returns None for CPU, "metal" for Apple Metal, or "cuda:0" for CUDA.
    pub fn candle_device(&self) -> Option<&'static str> {
        if !self.enabled {
            return None;
        }
        match self.available_backend {
            GpuBackend::Metal => Some("metal"),
            GpuBackend::Cuda => Some("cuda:0"),
            GpuBackend::None => None,
        }
    }
}

/// Detect available GPU backend based on OS and hardware.
fn detect_gpu_backend() -> GpuBackend {
    static DETECTED: OnceLock<GpuBackend> = OnceLock::new();

    *DETECTED.get_or_init(|| {
        #[cfg(all(feature = "embed-gpu", target_os = "macos"))]
        {
            // Check for Apple Silicon (M1/M2/M3/M4)
            if is_apple_silicon() {
                return GpuBackend::Metal;
            }
        }

        #[cfg(all(feature = "embed-cuda", any(target_os = "linux", target_os = "windows")))]
        {
            // Check for NVIDIA GPU via nvidia-smi or CUDA libraries
            if has_nvidia_gpu() {
                return GpuBackend::Cuda;
            }
        }

        GpuBackend::None
    })
}

/// Check if running on Apple Silicon (macOS).
#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn is_apple_silicon() -> bool {
    cfg!(target_arch = "aarch64")
}

/// Check for NVIDIA GPU availability.
#[cfg(any(target_os = "linux", target_os = "windows"))]
#[allow(dead_code)]
fn has_nvidia_gpu() -> bool {
    // Try to detect via CUDA libraries presence
    // For now, we assume if embed-cuda feature is compiled, CUDA is available
    // In production, you might want to check for nvidia-smi or CUDA libraries
    cfg!(feature = "embed-cuda")
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
#[allow(dead_code)]
fn has_nvidia_gpu() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_config_detect_returns_valid() {
        let config = GpuConfig::detect();
        match config.available_backend {
            GpuBackend::None => assert!(!config.enabled),
            GpuBackend::Metal => assert!(config.enabled),
            GpuBackend::Cuda => assert!(config.enabled),
        }
    }

    #[test]
    fn gpu_config_with_preference() {
        let config_with = GpuConfig::with_preference(true);
        let config_without = GpuConfig::with_preference(false);

        // If GPU is available, preference should be respected
        if matches!(config_with.available_backend, GpuBackend::Metal | GpuBackend::Cuda) {
            assert!(config_with.enabled);
            assert!(!config_without.enabled);
        } else {
            assert!(!config_with.enabled);
            assert!(!config_without.enabled);
        }
    }

    #[test]
    fn candle_device_returns_correct_string() {
        let config = GpuConfig::detect();
        match config.available_backend {
            GpuBackend::None => assert_eq!(config.candle_device(), None),
            GpuBackend::Metal => assert_eq!(config.candle_device(), Some("metal")),
            GpuBackend::Cuda => assert_eq!(config.candle_device(), Some("cuda:0")),
        }
    }
}
