# Solana Mixer - On-Chain Logic

## Overview
Solana Mixer is a privacy-preserving transaction system implemented on Solana using Anchor and zk-SNARKs. It allows users to deposit fixed amounts of SOL into a shielded pool and later withdraw them anonymously by proving ownership of deposited funds via zero-knowledge proofs. 
To interact with this program, look at the solana_mixer_cli on my github page.

## Features
- **Zero-Knowledge Proofs**: Uses Groth16 zk-SNARKs for anonymous withdrawals.
- **Merkle Tree Commitments**: Deposits are stored in a Merkle tree to prove membership efficiently.
- **Nullifier List**: Prevents double-spending by tracking used nullifiers.
- **Fixed Deposit Amount**: Each deposit is fixed at 0.1 SOL to ensure uniformity and prevent fingerprinting.

## Merkle Tree Implementation
- **Depth**: 4 (supports 16 deposits per pool)
- **Default Leaf**: `DEFAULT_LEAF_HASH` ensures new trees are initialized correctly.
- **Merkle Root Computation**: Uses Poseidon hashing to generate intermediate and final root nodes.

## Security Measures
- **Nullifier Check**: Prevents reuse of proofs by storing nullifier hashes.
- **Merkle Root Validation**: Ensures withdrawals reference a valid state of the deposit pool.
- **Anchor Security Features**: Uses program-derived addresses (PDAs) to manage funds securely.

## Dependencies
- **Solana Poseidon**: Cryptographic hash function optimized for zk-SNARK circuits.
- **Groth16-Solana**: Verifier for zk-SNARK proofs.
- **Anchor**: Framework for Solana smart contracts.

## Next Steps
- Improve scalability by implementing dynamic tree resizing.
- Reduce storage costs by optimizing nullifier list management.
- Explore support for variable deposit amounts while maintaining anonymity guarantees.

## License
MIT

