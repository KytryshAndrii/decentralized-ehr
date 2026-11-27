use crate::error::StoreError;

pub type Entry = (Vec<u8>, Vec<u8>);

pub trait Datastore {
    /// None only when the key is missing; read failures are errors.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError>;

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError>;

    fn has(&self, key: &[u8]) -> Result<bool, StoreError> {
        Ok(self.get(key)?.is_some())
    }

    /// Atomically writes the value only when the key is missing; false when it already exists.
    fn put_if_absent(&self, key: &[u8], value: &[u8]) -> Result<bool, StoreError>;

    /// A missing key is not an error.
    fn delete(&self, key: &[u8]) -> Result<(), StoreError>;

    fn entries_with_prefix(&self, prefix: &[u8]) -> Result<Vec<Entry>, StoreError>;

    /// Keys with their value sizes; a backend can override it so GC does not hold every block in RAM.
    fn key_sizes_with_prefix(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, usize)>, StoreError> {
        Ok(self
            .entries_with_prefix(prefix)?
            .into_iter()
            .map(|(key, value)| (key, value.len()))
            .collect())
    }
}
