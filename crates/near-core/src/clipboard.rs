/// Writes text to the platform clipboard without exposing a UI backend.
pub trait Clipboard: Send + Sync {
    /// Replaces the current platform clipboard text.
    ///
    /// # Errors
    ///
    /// Returns a platform-specific diagnostic when clipboard access is unavailable.
    fn set_text(&self, text: &str) -> Result<(), String>;
}
