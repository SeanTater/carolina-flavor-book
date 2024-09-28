use quick_cache::sync::Cache;
use quick_cache::Weighter;
use std::sync::Arc;

pub type GKCache = Arc<Cache<CacheQuery, CacheValue, ValueWeighter>>;

pub fn new_cache() -> GKCache {
    Arc::new(Cache::with_weighter(10, 50 << 20, ValueWeighter))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CacheQuery {
    Image { image_id: i64 },
    TagSearchPage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheValue {
    Image { image: Vec<u8> },
    TagSearchPage { page: String },
}

#[derive(Clone)]
pub struct ValueWeighter;

impl Weighter<CacheQuery, CacheValue> for ValueWeighter {
    fn weight(&self, _key: &CacheQuery, val: &CacheValue) -> u64 {
        match val {
            CacheValue::Image { image } => {
                tracing::info!("Image length: {}", image.len());
                image.len() as u64
            }
            CacheValue::TagSearchPage { page } => {
                tracing::info!("Page length: {}", page.len());
                page.len() as u64
            }
        }
    }
}
