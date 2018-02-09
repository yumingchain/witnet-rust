//Rust-Witnet is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
//Rust-Witnet is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
//You should have received a copy of the GNU General Public License
// along with Rust-Witnet. If not, see <http://www.gnu.org/licenses/>.
//
//This file is based on wallet/src/types.rs from
// <https://github.com/mimblewimble/grin>,
// originally developed by The Grin Developers and distributed under the
// Apache License, Version 2.0. You may obtain a copy of the License at
// <http://www.apache.org/licenses/LICENSE-2.0>.

use std::io;

use core::core::transaction;
use keychain;
use util::secp;

/// Wallet errors, mostly wrappers around underlying crypto or I/O errors.
#[derive(Debug)]
pub enum Error {
    NotEnoughFunds(u64),
    FeeDispute { sender_fee: u64, recipient_fee: u64 },
    FeeExceedsAmount { sender_amount: u64, recipient_fee: u64 },
    Keychain(keychain::Error),
    Transaction(transaction::Error),
    Secp(secp::Error),
    WalletData(String),
    /// An error in the format of the JSON structures exchanged by the wallet
    Format(String),
    /// An IO Error
    IOError(io::Error),
    /// Error with signatures during exchange
    Signature(String),
    /// Attempt to use duplicate transaction id in separate transactions
    DuplicateTransactionId,
    /// Wallet seed already exists
    WalletSeedExists,
    /// Other
    GenericError(String,)
}