/// Storage for application state documents that must survive process restarts.
pub trait StateDocumentStore: Send + Sync {
    /// Loads a complete state document.
    ///
    /// # Errors
    ///
    /// Returns storage failures without modifying persisted state.
    fn load(&self, document: &str) -> Result<Option<String>, String>;

    /// Atomically persists a complete state document.
    ///
    /// # Errors
    ///
    /// Returns a storage failure while retaining the previous recoverable document.
    fn persist(&self, document: &str, contents: &str) -> Result<(), String>;
}
