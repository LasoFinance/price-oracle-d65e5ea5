//! price-oracle (v1): a minimal mock price-oracle business on Solana.
//!
//! A publisher creates a price feed (e.g. "SOL/USD") and pushes prices with a
//! confidence interval. Each feed keeps its latest price and a ring buffer of the
//! last 16 updates. Consumers call `read_price` (directly, by CPI, or simulated
//! as a free view): it returns the price only if it is fresher than `max_age`
//! seconds, which is the guard every real oracle integration needs.
use anchor_lang::prelude::*;

declare_id!("D4vcBeHy6Z3jdJYaM27ERTwZfchqQSK7czXJ2JYUaMgQ");

pub const HISTORY: usize = 16;
pub const MAX_SYMBOL: usize = 16;

#[program]
pub mod price_oracle {
    use super::*;

    /// Publisher creates a feed. `decimals` = how many decimals `price` carries.
    pub fn create_feed(ctx: Context<CreateFeed>, symbol: String, decimals: u8) -> Result<()> {
        require!(!symbol.is_empty() && symbol.len() <= MAX_SYMBOL, OracleError::BadSymbol);
        require!(symbol.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'/' || b == b'-'), OracleError::BadSymbol);
        require!(decimals <= 12, OracleError::BadParams);
        let f = &mut ctx.accounts.feed;
        f.authority = ctx.accounts.authority.key();
        f.symbol = symbol;
        f.decimals = decimals;
        f.bump = ctx.bumps.feed;
        Ok(())
    }

    /// Publisher pushes a new price with its confidence (same decimals).
    pub fn update_price(ctx: Context<UpdatePrice>, price: i64, conf: u64) -> Result<()> {
        require!(price > 0, OracleError::BadParams);
        let clock = Clock::get()?;
        let f = &mut ctx.accounts.feed;
        let i = f.head as usize % HISTORY;
        f.hist_price[i] = price;
        f.hist_ts[i] = clock.unix_timestamp;
        f.head = ((i + 1) % HISTORY) as u8;
        f.price = price;
        f.conf = conf;
        f.updated_at = clock.unix_timestamp;
        f.slot = clock.slot;
        f.updates += 1;
        emit!(PriceUpdated { feed: f.key(), price, conf, ts: clock.unix_timestamp });
        Ok(())
    }

    /// Consumer read: returns the price, or fails if it is older than `max_age` seconds.
    pub fn read_price(ctx: Context<ReadPrice>, max_age: i64) -> Result<PriceData> {
        let f = &ctx.accounts.feed;
        require!(f.updates > 0, OracleError::NoPrice);
        let age = Clock::get()?.unix_timestamp - f.updated_at;
        require!(age <= max_age, OracleError::Stale);
        Ok(PriceData { price: f.price, conf: f.conf, decimals: f.decimals, updated_at: f.updated_at, age })
    }

    /// Hand the feed to another publisher.
    pub fn set_authority(ctx: Context<UpdatePrice>, new_authority: Pubkey) -> Result<()> {
        ctx.accounts.feed.authority = new_authority;
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct Feed {
    pub authority: Pubkey,
    #[max_len(16)]
    pub symbol: String,
    pub decimals: u8,
    pub price: i64,
    pub conf: u64,
    pub updated_at: i64,
    pub slot: u64,
    pub updates: u64,
    pub head: u8,
    pub hist_price: [i64; 16],
    pub hist_ts: [i64; 16],
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PriceData {
    pub price: i64,
    pub conf: u64,
    pub decimals: u8,
    pub updated_at: i64,
    pub age: i64,
}

#[event]
pub struct PriceUpdated {
    pub feed: Pubkey,
    pub price: i64,
    pub conf: u64,
    pub ts: i64,
}

#[derive(Accounts)]
#[instruction(symbol: String)]
pub struct CreateFeed<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(init, payer = authority, space = 8 + Feed::INIT_SPACE,
        seeds = [b"feed", authority.key().as_ref(), symbol.as_bytes()], bump)]
    pub feed: Account<'info, Feed>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdatePrice<'info> {
    pub authority: Signer<'info>,
    #[account(mut, has_one = authority)]
    pub feed: Account<'info, Feed>,
}

#[derive(Accounts)]
pub struct ReadPrice<'info> {
    pub feed: Account<'info, Feed>,
}

#[error_code]
pub enum OracleError {
    #[msg("Symbol must be 1-16 characters: letters, digits, '/' or '-'")]
    BadSymbol,
    #[msg("Invalid parameter")]
    BadParams,
    #[msg("This feed has no price yet")]
    NoPrice,
    #[msg("Price is older than the allowed max age")]
    Stale,
}
