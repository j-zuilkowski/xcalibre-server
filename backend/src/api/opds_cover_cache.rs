use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;
use tokio::sync::RwLock;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CoverVariant {
    Original,
    Thumb240,
    Large600,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CoverCacheKey {
    pub book_id: String,
    pub variant: CoverVariant,
    pub webp: bool,
}

#[derive(Clone)]
pub struct OpdsCoverCache {
    inner: Arc<RwLock<LruCache<CoverCacheKey, Vec<u8>>>>,
}

impl OpdsCoverCache {
    pub fn new(max_entries: usize) -> Self {
        let cap = NonZeroUsize::new(max_entries.max(1)).expect("non-zero capacity guaranteed");
        Self {
            inner: Arc::new(RwLock::new(LruCache::new(cap))),
        }
    }

    pub async fn get(&self, key: &CoverCacheKey) -> Option<Vec<u8>> {
        self.inner.write().await.get(key).cloned()
    }

    pub async fn put(&self, key: CoverCacheKey, bytes: Vec<u8>) {
        self.inner.write().await.put(key, bytes);
    }
}
