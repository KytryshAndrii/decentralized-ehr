use crate::error::StoreError;

pub trait Datastore {
    /// Zwraca None tylko gdy klucza nie ma; awarie odczytu są błędami.
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError>;

    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError>;

    /// Sprawdza istnienie wpisu, bez interpretacji wartości.
    fn has(&self, key: &[u8]) -> Result<bool, StoreError> {
        Ok(self.get(key)?.is_some())
    }

    /// Atomowo sprawdza istnienie klucza i zapisuje nowe dane.
    /// Zwraca false, gdy klucz już istnieje
    fn put_if_absent(&self, key: &[u8], value: &[u8]) -> Result<bool, StoreError>;

    /// Usuwa wpis; brak klucza nie jest błędem.
    fn delete(&self, key: &[u8]) -> Result<(), StoreError>;

    /// Zwraca wszystkie klucze zaczynające się od prefiksu, posortowane rosnąco.
    fn keys_with_prefix(&self, prefix: &[u8]) -> Result<Vec<Vec<u8>>, StoreError>;
}
