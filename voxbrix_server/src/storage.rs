use std::path::Path;
use sled::{
    Db,
    Tree,
};
pub use sled::{
    IVec,
    Error,
    transaction::{
        TransactionalTree,
        TransactionResult,
        ConflictableTransactionResult,
    },
};

trait Key {
    fn to_key<'a>(&self, buf: &'a mut Vec<u8>) -> &'a [u8];
    //fn from_key(key: &[u8]) -> Self;
}

impl Key for u32 {
    fn to_key<'a>(&self, buf: &'a mut Vec<u8>) -> &'a [u8] {
        buf.clear();
        buf.extend_from_slice(&self.to_be_bytes());
        buf.as_slice()
    }
}

pub struct Storage {
    db: Db,
}

impl Storage {
    pub async fn open<P>(path: P) -> Result<Self, Error>
    where
        P: 'static + AsRef<Path> + Send,
    {
        let db = blocking::unblock(|| sled::open(path)).await?;

        Ok(Self {
            db,
        })
    }

    pub async fn open_dataset<K>(&self, key: K) -> Result<Dataset, Error> 
    where
        K: 'static + AsRef<[u8]> + Send,
    {
        let db = self.db.clone();
        let tree = blocking::unblock(move || db.open_tree(key)).await?;

        Ok(Dataset {
            tree,
        })
    }
}

pub struct Dataset {
    tree: Tree,
}

impl Dataset {
    pub async fn transaction<F, A, E>(&self, f: F) -> TransactionResult<A, E>
    where
        F: 'static + Fn(&TransactionalTree) -> ConflictableTransactionResult<A, E> + Send, 
        E: 'static + Send,
        A: 'static + Send,
    {
        let tree = self.tree.clone();
        blocking::unblock(move || tree.transaction(f)).await
    }

    pub async fn get<K>(&self, key: K) -> Result<Option<IVec>, Error>
    where
        K: 'static + AsRef<[u8]> + Send,
    {
        let tree = self.tree.clone();
        blocking::unblock(move || tree.get(key)).await
    }
}
