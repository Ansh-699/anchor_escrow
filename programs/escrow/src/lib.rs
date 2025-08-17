#![allow(unexpected_cfgs)]
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{
    transfer, close_account, CloseAccount, Mint, TokenAccount, Token, Transfer,
};

declare_id!("9LmrznqPdcksZKcggQx6oBxVum6sSXRPLJUwfFhaekJ");

#[program]
pub mod escrow_anchor {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        id: u64,
        token_a_offered_amount: u64,
        token_b_wanted_amount: u64,
    ) -> Result<()> {
        let escrow = &mut ctx.accounts.escrow;
        escrow.id = id;
        escrow.maker = ctx.accounts.maker.key();
        escrow.token_mint_a = ctx.accounts.mint_a.key();
        escrow.token_mint_b = ctx.accounts.mint_b.key();
        escrow.token_a_offered_amount = token_a_offered_amount;
        escrow.token_b_wanted_amount = token_b_wanted_amount;
        escrow.vault = ctx.accounts.vault.key();
        escrow.bump = ctx.bumps.escrow;

        let transfer_accounts = Transfer {
            from: ctx.accounts.maker_ata_a.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
            authority: ctx.accounts.maker.to_account_info(),
        };
        let cpi_ctx =
            CpiContext::new(ctx.accounts.token_program.to_account_info(), transfer_accounts);
        transfer(cpi_ctx, token_a_offered_amount)?;

        Ok(())
    }

    pub fn refund(ctx: Context<Refund>) -> Result<()> {
        require_keys_eq!(ctx.accounts.maker.key(), ctx.accounts.escrow.maker);

        let seeds = &[
            b"escrow",
            ctx.accounts.escrow.maker.as_ref(),
            &ctx.accounts.escrow.id.to_le_bytes(),
            &[ctx.accounts.escrow.bump],
        ];
        let signer = &[&seeds[..]];

        let transfer_accounts = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.maker_ata_a.to_account_info(),
            authority: ctx.accounts.escrow.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_accounts,
            signer,
        );
        transfer(cpi_ctx, ctx.accounts.escrow.token_a_offered_amount)?;

        let close_accounts = CloseAccount {
            account: ctx.accounts.vault.to_account_info(),
            destination: ctx.accounts.maker.to_account_info(),
            authority: ctx.accounts.escrow.to_account_info(),
        };
        let cpi_close = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            close_accounts,
            signer,
        );
        close_account(cpi_close)?;
        Ok(())
    }

    pub fn take_escrow(ctx: Context<TakeEscrow>) -> Result<()> {
        require!(
            ctx.accounts.taker.key() != ctx.accounts.escrow.maker,
            EscrowError::InvalidTaker
        );

        let seeds = &[
            b"escrow",
            ctx.accounts.escrow.maker.as_ref(),
            &ctx.accounts.escrow.id.to_le_bytes(),
            &[ctx.accounts.escrow.bump],
        ];
        let signer = &[&seeds[..]];

        let transfer_b = Transfer {
            from: ctx.accounts.taker_ata_b.to_account_info(),
            to: ctx.accounts.maker_ata_b.to_account_info(),
            authority: ctx.accounts.taker.to_account_info(),
        };
        let cpi_b = CpiContext::new(ctx.accounts.token_program.to_account_info(), transfer_b);
        transfer(cpi_b, ctx.accounts.escrow.token_b_wanted_amount)?;

        let transfer_a = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.taker_ata_a.to_account_info(),
            authority: ctx.accounts.escrow.to_account_info(),
        };
        let cpi_a = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            transfer_a,
            signer,
        );
        transfer(cpi_a, ctx.accounts.escrow.token_a_offered_amount)?;

        let close_accounts = CloseAccount {
            account: ctx.accounts.vault.to_account_info(),
            destination: ctx.accounts.maker.to_account_info(),
            authority: ctx.accounts.escrow.to_account_info(),
        };
        let cpi_close = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            close_accounts,
            signer,
        );
        close_account(cpi_close)?;
        Ok(())
    }
}


#[derive(Accounts)]
#[instruction(id: u64)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,

    pub mint_a: Account<'info, Mint>,
    pub mint_b: Account<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = maker
    )]
    pub maker_ata_a: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = maker,
        space = 8 + EscrowState::INIT_SPACE,
        seeds = [b"escrow", maker.key().as_ref(), &id.to_le_bytes()],
        bump
    )]
    pub escrow: Account<'info, EscrowState>,

    #[account(
        init,
        payer = maker,
        associated_token::mint = mint_a,
        associated_token::authority = escrow
    )]
    pub vault: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Refund<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,

    #[account(
        mut,
        has_one = maker,
        close = maker
    )]
    pub escrow: Account<'info, EscrowState>,

    #[account(
        mut,
        associated_token::mint = escrow.token_mint_a,
        associated_token::authority = maker
    )]
    pub maker_ata_a: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault.key() == escrow.vault @ EscrowError::InvalidVault
    )]
    pub vault: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct TakeEscrow<'info> {
    #[account(mut)]
    pub taker: Signer<'info>,

    /// CHECK: This is validated against `escrow.maker`
    #[account(mut, constraint = maker.key() == escrow.maker @ EscrowError::InvalidMaker)]
    pub maker: UncheckedAccount<'info>,

    #[account(
        mut,
        has_one = maker,
        close = maker
    )]
    pub escrow: Account<'info, EscrowState>,

    #[account(
        mut,
        associated_token::mint = escrow.token_mint_a,
        associated_token::authority = taker
    )]
    pub taker_ata_a: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = escrow.token_mint_b,
        associated_token::authority = taker
    )]
    pub taker_ata_b: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = escrow.token_mint_b,
        associated_token::authority = maker
    )]
    pub maker_ata_b: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault.key() == escrow.vault @ EscrowError::InvalidVault
    )]
    pub vault: Account<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
}


#[account]
#[derive(InitSpace)]
pub struct EscrowState {
    pub id: u64,
    pub maker: Pubkey,
    pub token_mint_a: Pubkey,
    pub token_mint_b: Pubkey,
    pub token_a_offered_amount: u64,
    pub token_b_wanted_amount: u64,
    pub vault: Pubkey,
    pub bump: u8,
}


#[error_code]
pub enum EscrowError {
    #[msg("Invalid taker: Taker cannot be the same as the maker.")]
    InvalidTaker,
    #[msg("Invalid maker: The maker account provided does not match the escrow.")]
    InvalidMaker,
    #[msg("Invalid vault: The provided vault does not match the escrow state.")]
    InvalidVault,
}
