//! "Is anything actually playing on the default output device?" — used by the
//! recording stall watchdog before raising a system-audio stall alarm.
//!
//! On both macOS and Windows an idle output path can make the capture stream
//! deliver no buffers at all (sleeping process tap / idle WASAPI endpoint),
//! which is indistinguishable from a dead stream by heartbeat alone. When no
//! app is playing there is simply nothing to capture, so a "stall" in that
//! state must not alarm the user.

#[cfg(target_os = "macos")]
pub use super::core_audio::default_output_device_is_running;

#[cfg(target_os = "windows")]
pub use self::windows_imp::default_output_device_is_running;

/// Other platforms: no known idle-sleep capture behavior is handled here, so
/// keep the old "always alarm" behavior.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn default_output_device_is_running() -> bool {
    true
}

#[cfg(target_os = "windows")]
mod windows_imp {
    use log::warn;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, AudioSessionStateActive, IAudioSessionManager2, IMMDeviceEnumerator,
        MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoDecrementMTAUsage, CoIncrementMTAUsage, CLSCTX_ALL,
    };

    /// Whether any audio session on the default render endpoint is active —
    /// i.e. some app is actually playing audio right now.
    ///
    /// Errors fail open (`true`) so an undiagnosable state keeps the old
    /// "always alarm" behavior.
    pub fn default_output_device_is_running() -> bool {
        match query() {
            Ok(active) => active,
            Err(e) => {
                warn!("⚠️ WASAPI: failed to query output session state: {:?} — assuming active", e);
                true
            }
        }
    }

    fn query() -> windows::core::Result<bool> {
        unsafe {
            // WASAPI COM calls need an MTA on the calling thread; the watchdog
            // runs on a tokio task thread. CoIncrementMTAUsage nests safely.
            let cookie = CoIncrementMTAUsage()?;
            let result = query_inner();
            let _ = CoDecrementMTAUsage(cookie);
            result
        }
    }

    unsafe fn query_inner() -> windows::core::Result<bool> {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
        let sessions = manager.GetSessionEnumerator()?;
        let count = sessions.GetCount()?;
        for i in 0..count {
            let session = sessions.GetSession(i)?;
            if session.GetState()? == AudioSessionStateActive {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
