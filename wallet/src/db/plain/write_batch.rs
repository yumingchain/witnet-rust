use super::*;

#[derive(Default)]
pub struct PlainWriteBatch {
    batch: rocksdb::WriteBatch,
}

impl WriteBatch for PlainWriteBatch {
    fn put<K, V, Vref>(&mut self, key: &Key<K, V>, value: Vref) -> Result<()>
    where
        K: AsRef<[u8]>,
        V: serde::Serialize + ?Sized,
        Vref: Borrow<V>,
    {
        let bytes = bincode::serialize(value.borrow())?;

        self.batch.put(key, bytes)?;

        Ok(())
    }
}

impl Into<rocksdb::WriteBatch> for PlainWriteBatch {
    fn into(self) -> rocksdb::WriteBatch {
        self.batch
    }
}
