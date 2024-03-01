const {
  LOCALHOST,
  tmpLedgerDir,
} = require( '@metaplex-foundation/amman' );

module.exports = {
  validator: {
    killRunningValidators: true,
    programs: [/*
      { 
        label: 'Token Metadata Program',
        programId: programIds.metadata,
        deployPath: localDeployPath('mpl_token_metadata')
      },*/
    ],
    jsonRpcUrl: LOCALHOST,
    websocketUrl: 'ws://127.0.0.1:8900',
    commitment: 'confirmed',
    ledgerDir: tmpLedgerDir(),
    resetLedger: true,
    verifyFees: false,
    detached: process.env.CI != null,
  },
  relay: {
    enabled: process.env.CI == null,
    killlRunningRelay: true,
  },
  storage: {
    enabled: process.env.CI == null,
    storageId: 'mock-storage',
    clearOnStart: true,
  },
    // By default Amman will pull the account data from the accountsCluster (can be overridden on a per account basis)
    accountsCluster: 'https://jarrett-solana-7ba9.mainnet.rpcpool.com/8d890735-edf2-4a75-af84-92f7c9e31718',
    accounts: [
        {
          label: 'Token Metadata Program',
          accountId:'metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s',
          // marking executable as true will cause Amman to pull the executable data account as well automatically
          executable: true,
        },
        {
          label: 'stacc',
          accountId:'7ihN8QaTfNoDTRTQGULCzbUT3PHwPDTu5Brcu4iT2paP',
          // By default executable is false and is not required to be in the config
          // executable: false,
          
          // Providing a cluster here will override the accountsCluster field
          lamports: 10_000 * 10 ** 9
        }
      ]
  }
