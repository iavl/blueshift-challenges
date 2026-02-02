//! Refund instruction: maker gets token A back from vault; vault and escrow closed.

use pinocchio::{
    account_info::AccountInfo,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    ProgramResult,
};
use pinocchio_system::instructions::Transfer;
use pinocchio_token::instructions::{CloseAccount, TransferChecked};

use crate::state::Escrow;

const TOKEN_ACCOUNT_AMOUNT_OFFSET: usize = 64;

/// Refund accounts: maker, escrow, mint_a, vault, maker_ata_a, system_program, token_program.
pub struct RefundAccounts<'a> {
    pub maker: &'a AccountInfo,
    pub escrow: &'a AccountInfo,
    pub mint_a: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub maker_ata_a: &'a AccountInfo,
    pub system_program: &'a AccountInfo,
    pub token_program: &'a AccountInfo,
}

impl<'a> core::convert::TryFrom<&'a [AccountInfo]> for RefundAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [maker, escrow, mint_a, vault, maker_ata_a, system_program, token_program] =
            accounts else {
                return Err(ProgramError::NotEnoughAccountKeys);
            };

        if !maker.is_signer() {
            return Err(ProgramError::InvalidAccountOwner);
        }
        if !escrow.is_owned_by(&crate::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }
        let escrow_data = escrow.try_borrow_data()?;
        let escrow_state = Escrow::load(&*escrow_data)?;
        if escrow_state.maker != *maker.key() || escrow_state.mint_a != *mint_a.key() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        Ok(Self {
            maker,
            escrow,
            mint_a,
            vault,
            maker_ata_a,
            system_program,
            token_program,
        })
    }
}

pub struct Refund<'a> {
    pub accounts: RefundAccounts<'a>,
}

impl<'a> core::convert::TryFrom<&'a [AccountInfo]> for Refund<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        Ok(Self {
            accounts: RefundAccounts::try_from(accounts)?,
        })
    }
}

impl<'a> Refund<'a> {
    pub fn process(&mut self) -> ProgramResult {
        let escrow_data = self.accounts.escrow.try_borrow_data()?;
        let escrow = Escrow::load(&*escrow_data)?;
        let seed = escrow.seed;
        let bump = escrow.bump[0];
        drop(escrow_data);

        let seed_bytes = seed.to_le_bytes();
        let binding = [bump];
        let seeds = [
            Seed::from(b"escrow"),
            Seed::from(self.accounts.maker.key().as_ref()),
            Seed::from(seed_bytes.as_ref()),
            Seed::from(&binding),
        ];
        let signers = [Signer::from(&seeds)];

        let vault_data = self.accounts.vault.try_borrow_data()?;
        if vault_data.len() < TOKEN_ACCOUNT_AMOUNT_OFFSET + 8 {
            return Err(ProgramError::InvalidAccountData);
        }
        let vault_amount = u64::from_le_bytes(vault_data[TOKEN_ACCOUNT_AMOUNT_OFFSET..TOKEN_ACCOUNT_AMOUNT_OFFSET + 8].try_into().unwrap());
        drop(vault_data);

        let mint_a_data = self.accounts.mint_a.try_borrow_data()?;
        let decimals_a = if mint_a_data.len() > 44 { mint_a_data[44] } else { return Err(ProgramError::InvalidAccountData) };
        drop(mint_a_data);

        TransferChecked {
            from: self.accounts.vault,
            mint: self.accounts.mint_a,
            to: self.accounts.maker_ata_a,
            authority: self.accounts.escrow,
            amount: vault_amount,
            decimals: decimals_a,
        }
        .invoke_signed(&signers)?;

        CloseAccount {
            account: self.accounts.vault,
            destination: self.accounts.maker,
            authority: self.accounts.escrow,
        }
        .invoke_signed(&signers)?;

        Transfer {
            from: self.accounts.escrow,
            to: self.accounts.maker,
            lamports: self.accounts.escrow.lamports(),
        }
        .invoke_signed(&signers)?;

        Ok(())
    }
}
