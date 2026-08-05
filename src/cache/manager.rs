use std::sync::Arc;

use crate::cache::{stores::membership::MembershipCache, traits::CacheExecutor};

pub struct CacheManager<C: CacheExecutor> {
    pub membership: MembershipCache<C>,
}

impl<C: CacheExecutor> CacheManager<C> {
    pub fn new(chx: Arc<C>) -> Self {
        let membership = MembershipCache::new(chx.clone());

        Self { membership }
    }
}
