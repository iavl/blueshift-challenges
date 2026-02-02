//! PDA helpers for escrow.

use pinocchio::pubkey::{find_program_address, Pubkey};

/// Derive escrow PDA and bump. Seeds: [b"escrow", maker, seed_le_bytes].
pub fn find_escrow_address(maker: &Pubkey, seed: u64, program_id: &Pubkey) -> (Pubkey, u8) {
    find_program_address(
        &[b"escrow", maker.as_ref(), &seed.to_le_bytes()],
        program_id,
    )
}
