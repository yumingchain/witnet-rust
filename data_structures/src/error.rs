//! Error type definitions for the data structure module.

use failure::Fail;
use std::num::ParseIntError;

use crate::chain::{Epoch, Hash, HashParseError, OutputPointer, PublicKeyHash};

/// The error type for operations on a [`ChainInfo`](ChainInfo)
#[derive(Debug, PartialEq, Fail)]
pub enum ChainInfoError {
    /// Errors when try to use a None value for ChainInfo
    #[fail(display = "No ChainInfo loaded in ChainManager")]
    ChainInfoNotFound,
}

/// Error in builders functions
#[derive(Debug, PartialEq, Fail)]
pub enum BuildersError {
    /// No inventory vectors available to create a Inventory Announcement message
    #[fail(display = "No inventory vectors available to create a Inventory Announcement message")]
    NoInvVectorsAnnouncement,
    /// No inventory vectors available to create a Inventory Request message
    #[fail(display = "No inventory vectors available to create a Inventory Request message")]
    NoInvVectorsRequest,
}

/// The error type for operations on a [`Transaction`](Transaction)
#[derive(Debug, PartialEq, Fail)]
pub enum TransactionError {
    #[fail(display = "The transaction is invalid")]
    NotValidTransaction,
    /// The transaction creates value
    #[fail(display = "Transaction creates value (its fee is negative)")]
    NegativeFee,
    /// A transaction with the given hash wasn't found in a pool.
    #[fail(display = "A hash is missing in the pool (\"{}\")", hash)]
    PoolMiss { hash: Hash },
    /// An output with the given index wasn't found in a transaction.
    #[fail(display = "Output not found: {}", output)]
    OutputNotFound { output: OutputPointer },
    #[fail(display = "Data Request not found: {}", hash)]
    DataRequestNotFound { hash: Hash },
    #[fail(display = "The transaction signature is invalid")]
    InvalidSignature,
    #[fail(display = "Tally transaction is invalid")]
    InvalidTallyTransaction,
    #[fail(display = "Commit transaction has a invalid Proof of Eligibility")]
    InvalidDataRequestPoe,
    #[fail(
        display = "The data request eligibility claim VRF proof hash is greater than the target hash: {} > {}",
        vrf_hash, target_hash
    )]
    DataRequestEligibilityDoesNotMeetTarget { vrf_hash: Hash, target_hash: Hash },
    #[fail(display = "Invalid fee found: {}. Expected fee: {}", fee, expected_fee)]
    InvalidFee { fee: u64, expected_fee: u64 },
    #[fail(display = "Invalid Data Request reward: {}", reward)]
    InvalidDataRequestReward { reward: i64 },
    #[fail(
        display = "Invalid Data Request reward ({}) for this number of witnesses ({})",
        dr_value, witnesses
    )]
    InvalidDataRequestValue { dr_value: u64, witnesses: u16 },
    #[fail(display = "Data Request witnesses number is not enough")]
    InsufficientWitnesses,
    #[fail(
        display = "Mismatching between local tally ({:?}) and miner tally ({:?})",
        local_tally, miner_tally
    )]
    MismatchedConsensus {
        local_tally: Vec<u8>,
        miner_tally: Vec<u8>,
    },
    #[fail(
        display = "Mismatching number of signatures ({}) and inputs ({})",
        signatures_n, inputs_n
    )]
    MismatchingSignaturesNumber { signatures_n: u8, inputs_n: u8 },
    /// Transaction verification process failed.
    #[fail(
        display = "Failed to verify the signature of input {} in transaction {}: {}",
        index, hash, msg
    )]
    VerifyTransactionSignatureFail { hash: Hash, index: u8, msg: String },
    /// Signature not found
    #[fail(display = "Transaction signature not found")]
    SignatureNotFound,
    /// Public Key Hash does not match
    #[fail(
        display = "Public key hash mismatch: expected {} got {}",
        expected_pkh, signature_pkh
    )]
    PublicKeyHashMismatch {
        expected_pkh: PublicKeyHash,
        signature_pkh: PublicKeyHash,
    },
    /// Commit related to a reveal not found
    #[fail(display = "Commitment related to a reveal not found")]
    CommitNotFound,

    /// Commitment field in CommitTransaction does not match with RevealTransaction signature
    #[fail(
        display = "Commitment field in CommitTransaction does not match with RevealTransaction signature"
    )]
    MismatchedCommitment,
}

/// The error type for operations on a [`Block`](Block)
#[derive(Debug, PartialEq, Fail)]
pub enum BlockError {
    /// The block has no transactions in it.
    #[fail(display = "The block has no transactions")]
    Empty,
    /// The total value created by the mint transaction of the block,
    /// and the output value of the rest of the transactions, plus the
    /// block reward, don't add up
    #[fail(
        display = "The value of the mint transaction does not match the fees + reward of the block ({} != {} + {})",
        mint_value, fees_value, reward_value
    )]
    MismatchedMintValue {
        mint_value: u64,
        fees_value: u64,
        reward_value: u64,
    },
    #[fail(
        display = "Mint transaction has invalid epoch: mint {}, block {}",
        mint_epoch, block_epoch
    )]
    InvalidMintEpoch {
        mint_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[fail(display = "The block has an invalid PoE")]
    NotValidPoe,
    #[fail(
        display = "The block eligibility claim VRF proof hash is greater than the target hash: {} > {}",
        vrf_hash, target_hash
    )]
    BlockEligibilityDoesNotMeetTarget { vrf_hash: Hash, target_hash: Hash },
    #[fail(display = "The block has an invalid Merkle Tree")]
    NotValidMerkleTree,
    #[fail(
        display = "Block epoch from the future. Current epoch is: {}, block epoch is: {}",
        current_epoch, block_epoch
    )]
    BlockFromFuture {
        current_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[fail(
        display = "Ignoring block because its epoch ({}) is older than highest block checkpoint ({})",
        block_epoch, chain_epoch
    )]
    BlockOlderThanTip {
        chain_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[fail(
        display = "Ignoring block because previous hash (\"{}\") is unknown",
        hash
    )]
    PreviousHashNotKnown { hash: Hash },
    #[fail(
        display = "Block candidate's epoch differs from current epoch ({} != {})",
        block_epoch, current_epoch
    )]
    CandidateFromDifferentEpoch {
        current_epoch: Epoch,
        block_epoch: Epoch,
    },
    #[fail(
        display = "Commits in block ({}) are not equal to commits required ({})",
        commits, rf
    )]
    MismatchingCommitsNumber { commits: u32, rf: u32 },
    /// Block verification signature process failed.
    #[fail(display = "Failed to verify the signature of block {}", hash)]
    VerifySignatureFail { hash: Hash },
    /// Public Key Hash does not match
    #[fail(
        display = "Public key hash mismatch: VRF Proof PKH: {}, signature PKH: {}",
        proof_pkh, signature_pkh
    )]
    PublicKeyHashMismatch {
        proof_pkh: PublicKeyHash,
        signature_pkh: PublicKeyHash,
    },
}

#[derive(Debug, Fail)]
pub enum OutputPointerParseError {
    #[fail(display = "Failed to parse transaction hash: {}", _0)]
    Hash(HashParseError),
    #[fail(
        display = "Output pointer has the wrong format, expected '<transaction id>:<output index>'"
    )]
    MissingColon,
    #[fail(display = "Could not parse output index as an integer: {}", _0)]
    ParseIntError(ParseIntError),
}

/// The error type for operations on a [`Secp256k1Signature`](Secp256k1Signature)
#[derive(Debug, PartialEq, Fail)]
pub enum Secp256k1ConversionError {
    #[fail(
        display = "Failed to convert `witnet_data_structures::Signature` into `secp256k1::Signature`"
    )]
    FailSignatureConversion,
    #[fail(
        display = " Failed to convert `witnet_data_structures::PublicKey` into `secp256k1::PublicKey`"
    )]
    FailPublicKeyConversion,
    #[fail(
        display = " Failed to convert `secp256k1::PublicKey` into `witnet_data_structures::PublicKey`: public key must be 33 bytes long, is {}",
        size
    )]
    FailPublicKeyFromSlice { size: usize },
    #[fail(
        display = " Failed to convert `witnet_data_structures::SecretKey` into `secp256k1::SecretKey`"
    )]
    FailSecretKeyConversion,
}

/// The error type for operations on a [`DataRequestPool`](DataRequestPool)
#[derive(Debug, PartialEq, Fail)]
pub enum DataRequestError {
    /// Add commit method failed.
    #[fail(
        display = "Block contains a commitment for an unknown data request:\n\
                   Block hash: {}\n\
                   Transaction hash: {}\n\
                   Data request: {}",
        block_hash, tx_hash, dr_pointer
    )]
    AddCommitFail {
        block_hash: Hash,
        tx_hash: Hash,
        dr_pointer: Hash,
    },
    /// Add reveal method failed.
    #[fail(
        display = "Block contains a reveal for an unknown data request:\n\
                   Block hash: {}\n\
                   Transaction hash: {}\n\
                   Data request: {}",
        block_hash, tx_hash, dr_pointer
    )]
    AddRevealFail {
        block_hash: Hash,
        tx_hash: Hash,
        dr_pointer: Hash,
    },
    /// Add tally method failed.
    #[fail(
        display = "Block contains a tally for an unknown data request:\n\
                   Block hash: {}\n\
                   Transaction hash: {}\n\
                   Data request: {}",
        block_hash, tx_hash, dr_pointer
    )]
    AddTallyFail {
        block_hash: Hash,
        tx_hash: Hash,
        dr_pointer: Hash,
    },
    #[fail(display = "Received a commitment and Data Request is not in Commit stage")]
    NotCommitStage,
    #[fail(display = "Received a reveal and Data Request is not in Reveal stage")]
    NotRevealStage,
    #[fail(display = "Received a tally and Data Request is not in Tally stage")]
    NotTallyStage,
    #[fail(display = "Cannot persist unfinished data request (with no Tally)")]
    UnfinishedDataRequest,
}
