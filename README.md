# Solana Mixer - On-Chain Logic

## Overview
Solana Mixer is a privacy-preserving transaction system implemented on Solana using Anchor and zk-SNARKs. It allows users to deposit fixed amounts of SOL into a shielded pool and later withdraw them anonymously by proving ownership of deposited funds via zero-knowledge proofs. 
To interact with this program, look at the solana_mixer_cli on my github page.

## Features
- **Zero-Knowledge Proofs**: Uses Groth16 zk-SNARKs for anonymous withdrawals.
- **Merkle Tree Commitments**: Deposits are stored in a Merkle tree to prove membership efficiently.
- **Low cost**: The cost of withdrawal is capped at 0.001 sol to create a PDA to store the nullifier hash.

## Merkle Tree Implementation
- **Depth**: Is adjustable manually, currently at 8 for 256 leaves
- **Storage** : efficient storage with merkle mountain range, storing leaves permananently on the solana ledger with the client having to bring the correct leaves to have a valid transaction.
- **Padding**: Tree is padded with default leaves to have a fixed size tree but efficiently stored.
- **Default Leaf**: `DEFAULT_LEAF` ensures new trees are initialized correctly.
- **Merkle Root Computation**: Uses Poseidon hashing to generate intermediate and final root nodes.

## Security Measures
- **Nullifier Check**: Prevents reuse of proofs by creating a pda with nullifier as seed, it is important to make unique one.
- **Merkle Root Validation**: Ensures withdrawals reference a valid state of the deposit pool.
- **Anchor Security Features**: Uses program-derived addresses (PDAs) to manage funds securely.

## Dependencies
- **Solana Poseidon**: Cryptographic hash function optimized for zk-SNARK circuits.
- **Groth16-Solana**: Verifier for zk-SNARK proofs.
- **Anchor**: Framework for Solana smart contracts.

## Next Steps
- Explore support for variable deposit amounts while maintaining anonymity guarantees.
- Make the tree parameters modular to allow users to create their custom pools.

## License
MIT

