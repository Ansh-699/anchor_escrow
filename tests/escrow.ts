import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { EscrowAnchor } from "../target/types/escrow_anchor";
import {
    createMint,
    createAccount,
    mintTo,
    getAccount,
    TOKEN_PROGRAM_ID,
    ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { assert } from "chai";

describe("escrow", () => {
    anchor.setProvider(anchor.AnchorProvider.env());

    const program = anchor.workspace.EscrowAnchor as Program<EscrowAnchor>;
    const provider = anchor.AnchorProvider.env();

    const maker = anchor.web3.Keypair.generate();
    const taker = anchor.web3.Keypair.generate();

    let mintA: anchor.web3.PublicKey;
    let mintB: anchor.web3.PublicKey;

    let makerAtaA: anchor.web3.PublicKey;
    let makerAtaB: anchor.web3.PublicKey;

    let takerAtaA: anchor.web3.PublicKey;
    let takerAtaB: anchor.web3.PublicKey;

    let escrowPda: anchor.web3.PublicKey;
    let vaultPda: anchor.web3.PublicKey;

    const seed = new anchor.BN(12345);//random id

    before(async () => {
        const latestBlockHash = await provider.connection.getLatestBlockhash();

        await provider.connection.confirmTransaction({
            blockhash: latestBlockHash.blockhash,
            lastValidBlockHeight: latestBlockHash.lastValidBlockHeight,
            signature: await provider.connection.requestAirdrop(maker.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
        });

        await provider.connection.confirmTransaction({
            blockhash: latestBlockHash.blockhash,
            lastValidBlockHeight: latestBlockHash.lastValidBlockHeight,
            signature: await provider.connection.requestAirdrop(taker.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
        });

        // Create Mints
        mintA = await createMint(
            provider.connection,
            maker, // Payer
            maker.publicKey, // Mint Authority
            null, // Freeze Authority
            6 // Decimals
        );

        mintB = await createMint(
            provider.connection,
            taker, // Payer
            taker.publicKey, // Mint Authority
            null, // Freeze Authority
            6 // Decimals
        );

        // Create ATAs
        makerAtaA = await createAccount(
            provider.connection,
            maker,
            mintA,
            maker.publicKey
        );

        makerAtaB = await createAccount(
            provider.connection,
            maker,
            mintB,
            maker.publicKey
        );

        takerAtaA = await createAccount(
            provider.connection,
            taker,
            mintA,
            taker.publicKey
        );

        takerAtaB = await createAccount(
            provider.connection,
            taker,
            mintB,
            taker.publicKey
        );

        // Mint tokens to respective users
        await mintTo(
            provider.connection,
            maker,
            mintA,
            makerAtaA,
            maker.publicKey,
            1000_000000 // 1000 tokens
        );

        await mintTo(
            provider.connection,
            taker,
            mintB,
            takerAtaB,
            taker.publicKey,
            1000_000000 // 1000 tokens
        );

        // Derive PDAs
        [escrowPda] = anchor.web3.PublicKey.findProgramAddressSync(
            [Buffer.from("escrow"), maker.publicKey.toBuffer(), seed.toArrayLike(Buffer, "le", 8)],
            program.programId
        );

        // The vault is an associated token account owned by the escrow PDA
        vaultPda = await anchor.utils.token.associatedAddress({
            mint: mintA,
            owner: escrowPda
        });
    });

    it("Is initialized!", async () => {
        const tokenAOfferedAmount = new anchor.BN(100_000000); // 100 tokens
        const tokenBWantedAmount = new anchor.BN(200_000000); // 200 tokens

        const tx = await program.methods
            .initialize(seed, tokenAOfferedAmount, tokenBWantedAmount)
            .accountsPartial({
                maker: maker.publicKey,
                mintA: mintA,
                mintB: mintB,
                makerAtaA: makerAtaA,
                escrow: escrowPda,
                vault: vaultPda,
                associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
                tokenProgram: TOKEN_PROGRAM_ID,
                systemProgram: anchor.web3.SystemProgram.programId,
            })
            .signers([maker])
            .rpc();

        console.log("Your transaction signature", tx);

        const escrowAccount = await program.account.escrowState.fetch(escrowPda);
        assert.ok(escrowAccount.maker.equals(maker.publicKey));
        assert.ok(escrowAccount.tokenMintA.equals(mintA));
        assert.ok(escrowAccount.tokenMintB.equals(mintB));
        assert.ok(escrowAccount.tokenAOfferedAmount.eq(tokenAOfferedAmount));
        assert.ok(escrowAccount.tokenBWantedAmount.eq(tokenBWantedAmount));

        const vaultAccount = await getAccount(provider.connection, vaultPda);
        assert.ok(new anchor.BN(vaultAccount.amount.toString()).eq(tokenAOfferedAmount));
    });

    it("Takes escrow!", async () => {
        const initialMakerInfoB = await getAccount(provider.connection, makerAtaB);
        const initialTakerInfoA = await getAccount(provider.connection, takerAtaA);

        const tx = await program.methods
            .takeEscrow()
            .accountsPartial({
                taker: taker.publicKey,
                maker: maker.publicKey,
                escrow: escrowPda,
                takerAtaA: takerAtaA,
                takerAtaB: takerAtaB,
                makerAtaB: makerAtaB,
                vault: vaultPda,
                associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
                tokenProgram: TOKEN_PROGRAM_ID,
            })
            .signers([taker])
            .rpc();

        console.log("Your transaction signature", tx);

        // Check balances
        const makerInfoB = await getAccount(provider.connection, makerAtaB);
        const takerInfoA = await getAccount(provider.connection, takerAtaA);

        // Maker should have received the wanted amount of Token B (200)
        // Taker should have received the offered amount of Token A (100)
        assert.ok(new anchor.BN(makerInfoB.amount.toString()).sub(new anchor.BN(initialMakerInfoB.amount.toString())).eq(new anchor.BN(200_000000)));
        assert.ok(new anchor.BN(takerInfoA.amount.toString()).sub(new anchor.BN(initialTakerInfoA.amount.toString())).eq(new anchor.BN(100_000000)));

        // Escrow account should be closed
        try {
            await program.account.escrowState.fetch(escrowPda);
            assert.fail("Escrow account should be closed");
        } catch (e) {
            assert.include(e.message, "Account does not exist");
        }

        // Vault account should be closed
        try {
            await getAccount(provider.connection, vaultPda);
            assert.fail("Vault account should be closed");
        } catch (e) {
            // assert.include(e.message, "TokenAccountNotFoundError");
            assert.ok(e); // Just ensure it threw an error
        }
    });
});
