use core::convert::TryFrom;
use pinocchio::{
    account_info::Ref,
    instruction::{Seed, Signer},
    program_error::ProgramError,
    sysvars::{
        clock::Clock,
        instructions::{Instructions, IntrospectedInstruction},
        Sysvar,
    },
    ProgramResult,
};
use pinocchio_secp256r1_instruction::{Secp256r1Instruction, Secp256r1Pubkey};
use pinocchio_system::instructions::Transfer;

use pinocchio::account_info::AccountInfo;

pub struct WithdrawAccounts<'a> {
    pub payer: &'a AccountInfo,
    pub vault: &'a AccountInfo,
    pub instructions: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for WithdrawAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [payer, vault, instructions, _system_program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !payer.is_signer() {
            return Err(ProgramError::InvalidAccountOwner);
        }

        if !vault.is_owned_by(&pinocchio_system::ID) {
            return Err(ProgramError::InvalidAccountOwner);
        }

        if vault.lamports().eq(&0) {
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(Self {
            payer,
            vault,
            instructions,
        })
    }
}

pub struct WithdrawInstructionData {
    pub bump: [u8; 1],
}

impl<'a> TryFrom<&'a [u8]> for WithdrawInstructionData {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        Ok(Self {
            bump: [*data.first().ok_or(ProgramError::InvalidInstructionData)?],
        })
    }
}

pub struct Withdraw<'a> {
    pub accounts: WithdrawAccounts<'a>,
    pub instruction_data: WithdrawInstructionData,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Withdraw<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        let accounts = WithdrawAccounts::try_from(accounts)?;
        let instruction_data = WithdrawInstructionData::try_from(data)?;

        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> Withdraw<'a> {
    pub fn process(&mut self) -> ProgramResult {
        let instructions: Instructions<Ref<[u8]>> =
            Instructions::try_from(self.accounts.instructions)?;
        let ix: IntrospectedInstruction = instructions.get_instruction_relative(1)?;
        let secp256r1_ix = Secp256r1Instruction::try_from(&ix)?;
        if secp256r1_ix.num_signatures() != 1 {
            return Err(ProgramError::InvalidInstructionData);
        }
        let signer: Secp256r1Pubkey = *secp256r1_ix.get_signer(0)?;

        let message_data = secp256r1_ix.get_message_data(0)?;
        if message_data.len() < 32 + 8 {
            return Err(ProgramError::InvalidInstructionData);
        }
        let (payer, expiry) = message_data.split_at(32);
        if self.accounts.payer.key().as_ref().ne(payer) {
            return Err(ProgramError::InvalidAccountOwner);
        }

        let now = Clock::get()?.unix_timestamp;
        let expiry = i64::from_le_bytes(
            expiry[..8]
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
        );
        if now > expiry {
            return Err(ProgramError::InvalidInstructionData);
        }

        let seeds = [
            Seed::from(b"vault"),
            Seed::from(signer[..1].as_ref()),
            Seed::from(signer[1..].as_ref()),
            Seed::from(&self.instruction_data.bump),
        ];
        let signers = [Signer::from(&seeds)];

        Transfer {
            from: self.accounts.vault,
            to: self.accounts.payer,
            lamports: self.accounts.vault.lamports(),
        }
        .invoke_signed(&signers)
    }
}
