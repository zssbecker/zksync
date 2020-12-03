//! This module contains the definition of the fee token validator,
//! an entity which decides whether certain ERC20 token is suitable for paying fees.

// Built-in uses
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
// Workspace uses
use zksync_types::{
    tokens::{Token, TokenLike},
    Address,
};
// Local uses
use crate::utils::token_db_cache::TokenDBCache;

/// Fee token validator decides whether certain ERC20 token is suitable for paying fees.
#[derive(Debug, Clone)]
pub(crate) struct FeeTokenValidator<W> {
    tokens_cache: TokenCacheWrapper,
    /// List of tokens that aren't accepted to pay fees in.
    available_tokens: HashMap<Address, Instant>,
    available_time: Duration,
    available_amount: u64,
    watcher: W,
}

impl<W: TokenWatcher> FeeTokenValidator<W> {
    pub(crate) fn new(
        cache: impl Into<TokenCacheWrapper>,
        available_time: Duration,
        available_amount: u64,
        watcher: W,
    ) -> Self {
        Self {
            tokens_cache: cache.into(),
            available_tokens: Default::default(),
            available_time,
            available_amount,
            watcher,
        }
    }

    /// Returns `true` if token can be used to pay fees.
    pub(crate) async fn token_allowed(&mut self, token: TokenLike) -> anyhow::Result<bool> {
        let token = self.resolve_token(token).await?;
        if let Some(token) = token {
            self.check_token(token).await
        } else {
            // Unknown tokens aren't suitable for our needs, obviously.
            Ok(false)
        }
    }

    async fn resolve_token(&self, token: TokenLike) -> anyhow::Result<Option<Token>> {
        self.tokens_cache.get_token(token).await
    }

    async fn check_token(&mut self, token: Token) -> anyhow::Result<bool> {
        if let Some(last_token_check_time) = self.available_tokens.get(&token.address) {
            if last_token_check_time.elapsed() < self.available_time {
                return Ok(true);
            }
        }

        let amount = self.get_token_market_amount(&token).await?;
        if amount >= self.available_amount {
            self.available_tokens.insert(token.address, Instant::now());
            return Ok(true);
        }
        Ok(false)
    }
    async fn get_token_market_amount(&self, token: &Token) -> anyhow::Result<u64> {
        self.watcher.get_token_market_amount(token).await
    }
}

#[async_trait::async_trait]
pub trait TokenWatcher {
    async fn get_token_market_amount(&self, token: &Token) -> anyhow::Result<u64>;
}

pub struct UniswapTokenWatcher;

#[async_trait::async_trait]
impl TokenWatcher for UniswapTokenWatcher {
    async fn get_token_market_amount(&self, token: &Token) -> anyhow::Result<u64> {
        todo!()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum TokenCacheWrapper {
    DB(TokenDBCache),
    Memory(HashMap<TokenLike, Token>),
}

impl From<TokenDBCache> for TokenCacheWrapper {
    fn from(cache: TokenDBCache) -> Self {
        Self::DB(cache)
    }
}

impl From<HashMap<TokenLike, Token>> for TokenCacheWrapper {
    fn from(cache: HashMap<TokenLike, Token>) -> Self {
        Self::Memory(cache)
    }
}

impl TokenCacheWrapper {
    pub async fn get_token(&self, token_like: TokenLike) -> anyhow::Result<Option<Token>> {
        match self {
            Self::DB(cache) => cache.get_token(token_like).await,
            Self::Memory(cache) => Ok(cache.get(&token_like).cloned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    struct InMemoryTokenWatcher {
        amounts: HashMap<Address, u64>,
    }

    #[async_trait::async_trait]
    impl TokenWatcher for InMemoryTokenWatcher {
        async fn get_token_market_amount(&self, token: &Token) -> anyhow::Result<u64> {
            Ok(*self.amounts.get(&token.address).unwrap())
        }
    }

    #[tokio::test]
    async fn check_tokens() {
        let dai_token_address =
            Address::from_str("6b175474e89094c44da98b954eedeac495271d0f").unwrap();
        let dai_token = Token::new(1, dai_token_address, "DAI", 18);
        let phnx_token_address =
            Address::from_str("38A2fDc11f526Ddd5a607C1F251C065f40fBF2f7").unwrap();
        let phnx_token = Token::new(2, phnx_token_address, "PHNX", 18);

        let mut tokens = HashMap::new();
        tokens.insert(TokenLike::Address(dai_token_address), dai_token);
        tokens.insert(TokenLike::Address(phnx_token_address), phnx_token);

        let mut amounts = HashMap::new();
        amounts.insert(dai_token_address, 200);
        amounts.insert(phnx_token_address, 10);
        let mut validator = FeeTokenValidator::new(
            tokens,
            Duration::new(100, 0),
            100,
            InMemoryTokenWatcher { amounts },
        );

        let dai_allowed = validator
            .token_allowed(TokenLike::Address(dai_token_address))
            .await
            .unwrap();
        let phnx_allowed = validator
            .token_allowed(TokenLike::Address(phnx_token_address))
            .await
            .unwrap();
        assert_eq!(dai_allowed, true);
        assert_eq!(phnx_allowed, false);
    }
}
