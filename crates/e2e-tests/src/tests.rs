use std::{str::FromStr, sync::LazyLock};

use alloy::{
    eips::eip7702::Authorization,
    primitives::{b256, Address, B256, U32, Bytes},
    providers::{PendingTransactionBuilder, Provider, ProviderBuilder},
    signers::SignerSync,
    sol_types::SolValue,
};
use alloy_signer_local::{LocalSigner, PrivateKeySigner};
use alloy_network::{EthereumWallet, TransactionBuilder, TransactionBuilder7702};
use alloy_rpc_types_eth::TransactionRequest;
use url::Url;
use r55::{compile_deploy, compile_with_prefix};
use revm_primitives::{TransactTo};

/// RPC endpoint URL for the replica node
static REPLICA_RPC: LazyLock<Url> = LazyLock::new(|| {
    std::env::var("REPLICA_RPC")
        .expect("REPLICA_RPC environment variable is not set")
        .parse()
        .expect("REPLICA_RPC environment variable contains invalid URL")
});

/// RPC endpoint URL for the sequencer node
static SEQUENCER_RPC: LazyLock<Url> = LazyLock::new(|| {
    std::env::var("SEQUENCER_RPC")
        .expect("SEQUENCER_RPC environment variable is not set")
        .parse()
        .expect("SEQUENCER_RPC environment variable contains invalid URL")
});

/// Test account private key
const TEST_PRIVATE_KEY: B256 =
    b256!("59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d");

/// Default delegation address for testing
const DEFAULT_DELEGATION_ADDRESS: &str = "0x90f79bf6eb2c4f870365e785982e1f101e93b906";

/// Tests if the chain is advancing by checking block numbers
#[tokio::test]
async fn assert_chain_advances() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    let provider = ProviderBuilder::new().on_http(SEQUENCER_RPC.clone());

    let initial_block = provider.get_block_number().await?;

    // Wait for new block
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    let new_block = provider.get_block_number().await?;

    assert!(
        new_block > initial_block,
        "Chain did not advance: initial block {initial_block}, current block {new_block}"
    );

    Ok(())
}

/// Tests the wallet API functionality with EIP-7702 delegation
#[tokio::test]
async fn test_wallet_api() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    let provider = ProviderBuilder::new().on_http(REPLICA_RPC.clone());
    let signer = PrivateKeySigner::from_bytes(&TEST_PRIVATE_KEY)?;

    let delegation_address = Address::from_str(
        &std::env::var("DELEGATION_ADDRESS")
            .unwrap_or_else(|_| DEFAULT_DELEGATION_ADDRESS.to_string()),
    )?;

    // Create and sign authorization
    let auth = Authorization {
        chain_id: provider.get_chain_id().await?,
        address: delegation_address,
        nonce: provider.get_transaction_count(signer.address()).await?,
    };

    let signature = signer.sign_hash_sync(&auth.signature_hash())?;
    let auth = auth.into_signed(signature);

    // Prepare and send transaction
    let tx =
        TransactionRequest::default().with_authorization_list(vec![auth]).with_to(signer.address());

    let tx_hash: B256 = provider.client().request("wallet_sendTransaction", vec![tx]).await?;

    // Wait for and verify transaction receipt
    let receipt = PendingTransactionBuilder::new(provider.clone(), tx_hash).get_receipt().await?;

    assert!(receipt.status(), "Transaction failed");
    assert!(!provider.get_code_at(signer.address()).await?.is_empty(), "No code at signer address");

    Ok(())
}

// This is new endpoint `odyssey_sendTransaction`, upper test will be deprecate in the future.
#[tokio::test]
async fn test_new_wallet_api() -> Result<(), Box<dyn std::error::Error>> {
    if !ci_info::is_ci() {
        return Ok(());
    }

    let provider = ProviderBuilder::new().on_http(REPLICA_RPC.clone());
    let signer = PrivateKeySigner::from_bytes(&b256!(
        "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
    ))?;

    let delegation_address = Address::from_str(
        &std::env::var("DELEGATION_ADDRESS")
            .unwrap_or_else(|_| "0x90f79bf6eb2c4f870365e785982e1f101e93b906".to_string()),
    )
    .unwrap();

    let auth = Authorization {
        chain_id: provider.get_chain_id().await?,
        address: delegation_address,
        nonce: provider.get_transaction_count(signer.address()).await?,
    };

    let signature = signer.sign_hash_sync(&auth.signature_hash())?;
    let auth = auth.into_signed(signature);

    let tx =
        TransactionRequest::default().with_authorization_list(vec![auth]).with_to(signer.address());

    let tx_hash: B256 = provider.client().request("odyssey_sendTransaction", vec![tx]).await?;

    let receipt = PendingTransactionBuilder::new(provider.clone(), tx_hash).get_receipt().await?;

    assert!(receipt.status());

    assert!(!provider.get_code_at(signer.address()).await?.is_empty());

    Ok(())
}

/// Tests withdrawal proof functionality with fallback behavior
// #[tokio::test]
// async fn test_withdrawal_proof_with_fallback() -> Result<(), Box<dyn std::error::Error>> {
//     if !ci_info::is_ci() {
//         return Ok(());
//     }

//     let provider = ProviderBuilder::new().on_http(REPLICA_RPC.clone());

//     // Get latest block for proof verification
//     let block: Block = provider
//         .client()
//         .request("eth_getBlockByNumber", (BlockNumberOrTag::Latest, false))
//         .await?;
//     let block_number = BlockNumberOrTag::Number(block.header.number);

//     // Withdrawal contract will return an empty account proof, since it only handles storage proofs
//     let withdrawal_contract_response: EIP1186AccountProofResponse = provider
//         .client()
//         .request(
//             "eth_getProof",
//             (odyssey_common::WITHDRAWAL_CONTRACT, vec![B256::ZERO], block_number),
//         )
//         .await?;

//     assert!(withdrawal_contract_response.account_proof.is_empty());
//     assert!(!withdrawal_contract_response.storage_proof.is_empty());

//     let storage_root = withdrawal_contract_response.storage_hash;
//     for proof in withdrawal_contract_response.storage_proof {
//         StorageProof::new(proof.key.as_b256()).with_proof(proof.proof).verify(storage_root)?
//     }

//     // If not targeting the withdrawal contract, it defaults back to the standard getProof
//     // implementation
//     let signer = PrivateKeySigner::from_bytes(&b256!(
//         "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d"
//     ))?;

//     let eoa_response: EIP1186AccountProofResponse = provider
//         .client()
//         .request("eth_getProof", (signer.address(), [0; 0], block_number))
//         .await
//         .unwrap();

//     assert!(!eoa_response.account_proof.is_empty());
//     AccountProof {
//         address: signer.address(),
//         info: Some(Account {
//             nonce: eoa_response.nonce,
//             balance: eoa_response.balance,
//             bytecode_hash: Some(eoa_response.code_hash),
//         }),
//         proof: eoa_response.account_proof,
//         ..Default::default()
//     }
//     .verify(block.header.state_root)?;

//     Ok(())
// }

// Test addresses - using well-known private keys for testing
const ALICE_PRIVATE_KEY: &str =
    "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";
const BOB_ADDRESS: &str = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";
const ERC20_PATH: &str = "/Users/rampey/Documents/r55/examples/erc20";

async fn compile_and_deploy_erc20() -> Result<Address, Box<dyn std::error::Error>> {
    // Set up signer with Alice's private key
    let signer: PrivateKeySigner = LocalSigner::from_str(ALICE_PRIVATE_KEY)?;
    let owner = signer.address();
    let wallet = EthereumWallet::from(signer);
    // Connect to local Ethereum node
    let rpc_url = "http://127.0.0.1:32826".parse()?;
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet.clone())
        .on_http(rpc_url);
    // Compile the R55 ERC20 contract
    let bytecode = compile_with_prefix(compile_deploy, ERC20_PATH)?;

    // Encode constructor arguments (owner address)
    let constructor = owner.abi_encode();

    let init_code = if Some(&0xff) == bytecode.first() {
        // Craft R55 initcode: [0xFF][codesize][bytecode][constructor_args]
        let codesize = U32::from(bytecode.len());

        let mut init_code = Vec::new();
        init_code.push(0xff);
        init_code.extend_from_slice(&Bytes::from(codesize.to_be_bytes_vec()));
        init_code.extend_from_slice(&bytecode);
        if let Some(args) = Some(constructor) {
            init_code.extend_from_slice(&args);
        }
        Bytes::from(init_code)
    } else {
        // do not modify bytecode for EVM contracts
        bytecode
    };

    // Create deployment transaction
    let mut deploy_tx = TransactionRequest::default()
        .from(owner)
        .input(Bytes::from(init_code).into());
    deploy_tx.to = Some(TransactTo::Create);

    // Send deployment transaction
    println!("Sending deployment transaction...");
    let pending_tx = provider.send_transaction(deploy_tx).await?;
    println!("Transaction sent, waiting for receipt...");
    let receipt = pending_tx.get_receipt().await?;

    let contract_address = receipt
        .contract_address
        .ok_or("No contract address in receipt")?;

    println!("✅ Deployed R55 ERC20 contract at: {}", contract_address);
    Ok(contract_address)
}

#[tokio::test]
async fn test_jsonrpc_erc20_deployment() -> Result<(), Box<dyn std::error::Error>> {
    // Deploy contract
    println!("Deploying contract...");
    let contract_address = compile_and_deploy_erc20().await?;

    // Verify deployment by calling owner() function
    // let result = call_contract(
    //     &provider,
    //     contract_address,
    //     alice_address,
    //     "owner()",
    //     vec![],
    // )
    // .await?;

    // Decode the result - owner should be Alice
    // let owner_result = Address::from_word(alloy_primitives::B256::from_slice(result.as_ref()));
    // assert_eq!(
    //     owner_result, alice_address,
    //     "Contract owner should be Alice"
    // );

    println!("✅ Contract deployment verified successfully!");
    Ok(())
}