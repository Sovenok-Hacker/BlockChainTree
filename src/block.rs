use crate::dump_headers::Headers;
use crate::errors::*;
use crate::merkletree::MerkleTree;
use crate::tools;
use crate::transaction::{Transaction, Transactionable};
use byteorder::{BigEndian, ReadBytesExt};
use num_bigint::BigUint;
use std::convert::TryInto;
use std::mem::transmute;
//use std::mem::transmute_copy;

use error_stack::{Report, Result, ResultExt};

#[macro_export]

macro_rules! bytes_to_u64 {
    ($buffer:expr,$buffer_index:expr) => {
        (&$buffer[$buffer_index..$buffer_index + 8])
            .read_u64::<BigEndian>()
            .unwrap()
    };
}

static ALREADY_SET: &str = "data is already set";

#[derive(Debug, Clone)]
pub struct BasicInfo {
    timestamp: u64,
    pow: BigUint,
    previous_hash: [u8; 32],
    //current_hash: [u8; 32],
    height: u64,
    difficulty: [u8; 32],
}

impl BasicInfo {
    pub fn new(
        //miner:[u8;33],
        timestamp: u64,
        pow: BigUint,
        previous_hash: [u8; 32],
        //current_hash: [u8; 32],
        height: u64,
        difficulty: [u8; 32],
    ) -> BasicInfo {
        BasicInfo {
            //miner:miner,
            timestamp,
            pow,
            previous_hash,
            //current_hash,
            height,
            difficulty,
        }
    }

    pub fn get_dump_size(&self) -> usize {
        8 + tools::bigint_size(&self.pow) + 32 + 32 + 8 + 32
    }
    pub fn dump(&self, buffer: &mut Vec<u8>) -> Result<(), BlockError> {
        // dumping timestamp
        for byte in self.timestamp.to_be_bytes().iter() {
            buffer.push(*byte);
        }

        // dumping previous hash
        for byte in self.previous_hash.iter() {
            buffer.push(*byte);
        }

        // // dumping current hash
        // for byte in self.current_hash.iter() {
        //     buffer.push(*byte);
        // }

        // dumping height
        for byte in self.height.to_be_bytes().iter() {
            buffer.push(*byte);
        }

        // dumping difficulty
        buffer.extend(self.difficulty);

        // dumping PoW
        tools::dump_biguint(&self.pow, buffer)
            .change_context(BlockError::BasicInfo(BasicInfoErrorKind::Dump))?;

        Ok(())
    }

    pub fn parse(data: &[u8]) -> Result<BasicInfo, BlockError> {
        let mut index: usize = 0;

        if data.len() <= 112 {
            return Err(
                Report::new(BlockError::BasicInfo(BasicInfoErrorKind::Parse))
                    .attach_printable("data <= 112"),
            );
        }

        // parsing timestamp
        let timestamp = bytes_to_u64!(data, index);
        index += 8;

        // parsing previous hash
        let previous_hash: [u8; 32] =
            unsafe { data[index..index + 32].try_into().unwrap_unchecked() };
        index += 32;

        // // parsing current hash
        // let current_hash: [u8; 32] =
        //     unsafe { data[index..index + 32].try_into().unwrap_unchecked() };
        // index += 32;

        // parsing height
        let height: u64 = bytes_to_u64!(data, index);
        index += 8;

        // parsing difficulty
        let difficulty: [u8; 32] = unsafe { data[index..index + 32].try_into().unwrap_unchecked() };
        index += 32;

        // parsing PoW
        let (pow, _) = tools::load_biguint(&data[index..])
            .change_context(BlockError::BasicInfo(BasicInfoErrorKind::Parse))
            .attach_printable("failed to parse PoW")?;

        Ok(BasicInfo {
            timestamp,
            pow,
            previous_hash,
            //current_hash,
            height,
            difficulty,
        })
    }
}

#[derive(Debug)]
pub struct TransactionBlock {
    transactions: Vec<[u8; 32]>,
    fee: BigUint,
    merkle_tree: Option<MerkleTree>,
    merkle_tree_root: [u8; 32],
    default_info: BasicInfo,
}

impl TransactionBlock {
    pub fn new(
        transactions: Vec<[u8; 32]>,
        fee: BigUint,
        default_info: BasicInfo,
        merkle_tree_root: [u8; 32],
    ) -> TransactionBlock {
        TransactionBlock {
            transactions,
            fee,
            merkle_tree: None,
            default_info,
            merkle_tree_root,
        }
    }

    pub fn merkle_tree_is_built(&self) -> bool {
        self.merkle_tree.is_some()
    }

    pub fn build_merkle_tree(&mut self) -> Result<(), BlockError> {
        let mut new_merkle_tree = MerkleTree::new();

        let res = new_merkle_tree.add_objects(&self.transactions);
        if !res {
            return Err(Report::new(BlockError::TransactionBlock(
                TxBlockErrorKind::BuildingMerkleTree,
            )));
        }
        self.merkle_tree = Some(new_merkle_tree);
        Ok(())
    }

    pub fn check_merkle_tree(&mut self) -> Result<bool, BlockError> {
        // build merkle tree if not built
        if !self.merkle_tree_is_built() {
            self.build_merkle_tree()?;
        }

        // transmute computed root into 4 u64 bytes
        let constructed_tree_root_raw = self.merkle_tree.as_ref().unwrap().get_root();
        let constructed_tree_root_raw_root: &[u64; 4] =
            unsafe { transmute(constructed_tree_root_raw) };

        // transmute root into 4 u64 bytes
        let root: &[u64; 4] = unsafe { transmute(&self.merkle_tree_root) };

        for (a, b) in root.iter().zip(constructed_tree_root_raw_root.iter()) {
            if *a != *b {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn get_dump_size(&self) -> usize {
        let mut size: usize = 1;
        size += tools::bigint_size(&self.fee);
        size += 32;
        size += self.default_info.get_dump_size();
        size += self.transactions.len() * 32;

        size
    }

    pub fn dump_with_transactions(
        &self,
        transactions: &[impl Transactionable],
    ) -> Result<Vec<u8>, BlockError> {
        let size: usize = self.get_dump_size();

        let mut to_return: Vec<u8> = Vec::with_capacity(size);

        //header
        to_return.push(Headers::TransactionBlock as u8);

        // merkle tree root
        to_return.extend(self.merkle_tree_root.iter());

        // default info
        self.default_info
            .dump(&mut to_return)
            .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Dump))?;

        // fee
        tools::dump_biguint(&self.fee, &mut to_return)
            .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Dump))?;

        // amount of transactions
        let amount_of_transactions = if self.transactions.len() > 0xFFFF {
            return Err(
                Report::new(BlockError::TransactionBlock(TxBlockErrorKind::Dump))
                    .attach_printable(format!("transactions: {}", self.transactions.len())),
            );
        } else {
            self.transactions.len() as u16
        };

        to_return.extend(amount_of_transactions.to_be_bytes().iter());

        // transactions/tokens
        for transaction in transactions.iter() {
            // size of transaction
            let size_of_transaction: u32 = transaction.get_dump_size() as u32;
            to_return.extend(size_of_transaction.to_be_bytes().iter());

            for byte in transaction.dump().unwrap().iter() {
                to_return.push(*byte);
            }
        }

        Ok(to_return)
    }

    pub fn dump(&self) -> Result<Vec<u8>, BlockError> {
        let size: usize = self.get_dump_size();

        let mut to_return: Vec<u8> = Vec::with_capacity(size);

        //header
        to_return.push(Headers::TransactionBlock as u8);

        // merkle tree root
        to_return.extend(self.merkle_tree_root.iter());

        // default info
        self.default_info
            .dump(&mut to_return)
            .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Dump))?;

        // fee
        tools::dump_biguint(&self.fee, &mut to_return)
            .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Dump))?;

        // transactions hashes
        for hash in &self.transactions {
            to_return.extend(hash);
        }

        Ok(to_return)
    }

    pub fn parse(data: &[u8], block_size: u32) -> Result<TransactionBlock, BlockError> {
        let mut offset: usize = 0;

        // merkle tree root
        let merkle_tree_root: [u8; 32] = data[..32].try_into().unwrap();
        offset += 32; // inc offset

        // default info
        let default_info = BasicInfo::parse(&data[offset..])
            .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Parse))?;

        offset += default_info.get_dump_size(); // inc offset

        // fee
        let (fee, _offset) = tools::load_biguint(&data[offset..])
            .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Parse))?;

        offset += _offset; // inc offset

        if (data.len() - offset) % 32 != 0 {
            return Err(BlockError::TransactionBlock(TxBlockErrorKind::Parse).into());
        }

        // parse transaction hashes
        let transactions: Vec<[u8; 32]> = data[offset..]
            .chunks_exact(32)
            .map(|hash| unsafe { hash.try_into().unwrap_unchecked() })
            .collect();

        Ok(TransactionBlock {
            transactions,
            fee,
            merkle_tree: None,
            merkle_tree_root,
            default_info,
        })
    }

    pub fn parse_with_transactions(
        data: &[u8],
        block_size: u32,
    ) -> Result<(TransactionBlock, Vec<Box<dyn Transactionable>>), BlockError> {
        let mut offset: usize = 0;

        // merkle tree root
        let merkle_tree_root: [u8; 32] = data[..32].try_into().unwrap();
        offset += 32; // inc offset

        // default info
        let default_info = BasicInfo::parse(&data[offset..])
            .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Parse))?;

        offset += default_info.get_dump_size(); // inc offset

        // fee
        let (fee, _offset) = tools::load_biguint(&data[offset..])
            .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Parse))?;

        offset += _offset; // inc offset

        // transactions
        let amount_of_transactions: u16 =
            u16::from_be_bytes(data[offset..offset + 2].try_into().unwrap());
        offset += 2; // inc offset

        let mut transactions: Vec<Box<dyn Transactionable>> =
            Vec::with_capacity(amount_of_transactions as usize);

        for _ in 0..amount_of_transactions {
            let transaction_size: u32 =
                u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap()) - 1;

            offset += 4; // inc offset

            let header = Headers::from_u8(data[offset])
                .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Parse))?;
            offset += 1;

            //let mut trtk: TransactionToken = TransactionToken::new(None, None);
            let tr = match header {
                Headers::Transaction => Transaction::parse(
                    &data[offset..offset + (transaction_size as usize)],
                    transaction_size as u64,
                )
                .change_context(BlockError::TransactionBlock(TxBlockErrorKind::Parse))?,
                Headers::Token => {
                    return Err(Report::new(BlockError::NotImplemented(
                        NotImplementedKind::Token,
                    )));
                }
                _ => {
                    return Err(Report::new(BlockError::HeaderError(
                        DumpHeadersErrorKind::WrongHeader,
                    )));
                }
            };

            offset += transaction_size as usize; // inc offset

            transactions.push(Box::new(tr));
        }

        if offset != block_size as usize {
            return Err(Report::new(BlockError::TransactionBlock(
                TxBlockErrorKind::Parse,
            )));
        }

        let transactions_hashes: Vec<[u8; 32]> = transactions.iter().map(|tr| tr.hash()).collect();

        Ok((
            TransactionBlock::new(transactions_hashes, fee, default_info, merkle_tree_root),
            transactions,
        ))
    }

    pub fn hash(&self) -> Result<[u8; 32], BlockError> {
        let dump: Vec<u8> = self.dump()?;

        Ok(tools::hash(&dump))
    }
}

impl MainChainBlock for TransactionBlock {
    fn hash(&self) -> Result<[u8; 32], BlockError> {
        self.hash()
    }

    fn get_dump_size(&self) -> usize {
        self.get_dump_size()
    }

    fn dump(&self) -> Result<Vec<u8>, BlockError> {
        self.dump()
    }
}

pub struct TokenBlock {
    pub default_info: BasicInfo,
    pub token_signature: String,
    pub payment_transaction: Transaction,
}

impl TokenBlock {
    pub fn new(
        default_info: BasicInfo,
        token_signature: String,
        payment_transaction: Transaction,
    ) -> TokenBlock {
        TokenBlock {
            default_info,
            token_signature,
            payment_transaction,
        }
    }

    pub fn get_dump_size(&self) -> usize {
        self.default_info.get_dump_size()
            + self.token_signature.len()
            + 1
            + self.payment_transaction.get_dump_size()
    }

    pub fn dump(&self) -> Result<Vec<u8>, BlockError> {
        let dump_size: usize = self.get_dump_size();

        let mut dump: Vec<u8> = Vec::with_capacity(dump_size);

        // header
        dump.push(Headers::TokenBlock as u8);

        // // dumping token signature
        // for byte in self.token_signature.as_bytes().iter(){
        //     dump.push(*byte);
        // }
        // dump.push(0);

        // dumping payment transaction
        let transaction_len: u32 = self.payment_transaction.get_dump_size() as u32;
        dump.extend(transaction_len.to_be_bytes().iter());

        let result = self
            .payment_transaction
            .dump()
            .change_context(BlockError::TokenBlock(TokenBlockErrorKind::Dump))?;

        dump.extend(result);

        // dumping default info
        self.default_info
            .dump(&mut dump)
            .change_context(BlockError::TokenBlock(TokenBlockErrorKind::Dump))?;

        Ok(dump)
    }

    pub fn parse(data: &[u8], block_size: u32) -> Result<TokenBlock, BlockError> {
        let mut offset: usize = 0;
        // parsing token signature
        let token_signature: String = String::new();
        // for byte in data{
        //     offset += 1;
        //     if *byte == 0{
        //         break;
        //     }
        //     token_signature.push(*byte as char);
        // }

        // parsing transaction
        let transaction_size: u32 =
            u32::from_be_bytes(data[offset..offset + 4].try_into().unwrap());
        offset += 4;

        if data[offset] != Headers::Transaction as u8 {
            return Err(Report::new(BlockError::TokenBlock(
                TokenBlockErrorKind::Parse,
            )));
        }
        offset += 1;

        let payment_transaction = Transaction::parse(
            &data[offset..offset + transaction_size as usize],
            (transaction_size - 1) as u64,
        )
        .attach_printable("Error parsing token block: couldn't parse transaction")
        .change_context(BlockError::TokenBlock(TokenBlockErrorKind::Parse))?;

        offset += (transaction_size - 1) as usize;

        // parsing basic info
        let default_info = BasicInfo::parse(&data[offset..block_size as usize])
            .attach_printable("Error parsing token block: couldn't parse basic info")
            .change_context(BlockError::TokenBlock(TokenBlockErrorKind::Parse))?;

        offset += default_info.get_dump_size();

        if offset != block_size as usize {
            return Err(Report::new(BlockError::TokenBlock(
                TokenBlockErrorKind::Parse,
            )));
        }

        Ok(TokenBlock {
            default_info,
            token_signature,
            payment_transaction,
        })
    }

    pub fn hash(&self) -> Result<[u8; 32], BlockError> {
        let dump: Vec<u8> = self.dump().unwrap();

        Ok(tools::hash(&dump))
    }
}

pub struct SummarizeBlock {
    default_info: BasicInfo,
    founder_transaction: [u8; 32],
}

impl SummarizeBlock {
    pub fn new(default_info: BasicInfo, founder_transaction: [u8; 32]) -> SummarizeBlock {
        SummarizeBlock {
            default_info,
            founder_transaction,
        }
    }

    pub fn get_dump_size(&self) -> usize {
        1 // header
        +self.default_info.get_dump_size()
        +32
    }

    pub fn dump(&self) -> Result<Vec<u8>, BlockError> {
        let mut to_return: Vec<u8> = Vec::with_capacity(self.get_dump_size());

        // header
        to_return.push(Headers::SummarizeBlock as u8);

        // dump transaction
        to_return.extend(self.founder_transaction);

        // dump basic info
        self.default_info.dump(&mut to_return)?;

        Ok(to_return)
    }

    pub fn parse(data: &[u8]) -> Result<SummarizeBlock, BlockError> {
        if data.len() <= 32 {
            return Err(
                Report::new(BlockError::SummarizeBlock(SummarizeBlockErrorKind::Parse))
                    .attach_printable("data length <= 32"),
            );
        }

        // parse transaction
        let founder_transaction: [u8; 32] = unsafe { data[0..32].try_into().unwrap_unchecked() };

        // parse default info
        let default_info = BasicInfo::parse(&data[32..])
            .change_context(BlockError::SummarizeBlock(SummarizeBlockErrorKind::Parse))?;

        Ok(SummarizeBlock {
            default_info,
            founder_transaction,
        })
    }

    pub fn hash(&self) -> Result<[u8; 32], BlockError> {
        let result = self
            .dump()
            .change_context(BlockError::SummarizeBlock(SummarizeBlockErrorKind::Hash));

        let dump: Vec<u8> = unsafe { result.unwrap_unchecked() };

        Ok(tools::hash(&dump))
    }
}

impl MainChainBlock for SummarizeBlock {
    fn hash(&self) -> Result<[u8; 32], BlockError> {
        self.hash()
    }

    fn get_dump_size(&self) -> usize {
        self.get_dump_size()
    }

    fn dump(&self) -> Result<Vec<u8>, BlockError> {
        self.dump()
    }
}

pub trait MainChainBlock {
    fn hash(&self) -> Result<[u8; 32], BlockError>;
    fn get_dump_size(&self) -> usize;
    fn dump(&self) -> Result<Vec<u8>, BlockError>;
}
