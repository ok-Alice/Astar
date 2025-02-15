// This file is part of Astar.

// Copyright (C) 2019-2023 Stake Technologies Pte.Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later

// Astar is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Astar is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Astar. If not, see <http://www.gnu.org/licenses/>.

//! Runtime integration tests setup & imports.

pub use frame_support::{
    assert_noop, assert_ok,
    traits::{GenesisBuild, OnFinalize, OnIdle, OnInitialize},
    weights::Weight,
};
pub use pallet_evm::AddressMapping;
pub use sp_core::{H160, H256, U256};
pub use sp_io::hashing::keccak_256;
pub use sp_runtime::{AccountId32, MultiAddress};

pub use astar_primitives::{
    dapp_staking::CycleConfiguration, evm::UnifiedAddressMapper, BlockNumber,
};

#[cfg(feature = "shibuya")]
pub use shibuya::*;
#[cfg(feature = "shibuya")]
mod shibuya {
    use super::*;
    use parity_scale_codec::Decode;
    pub use shibuya_runtime::*;

    /// 1 SBY.
    pub const UNIT: Balance = SBY;

    pub fn alith_secret_key() -> libsecp256k1::SecretKey {
        libsecp256k1::SecretKey::parse(&keccak_256(b"Alith")).unwrap()
    }

    /// H160 address mapped to `ALICE`.
    pub fn alith() -> H160 {
        UnifiedAccounts::eth_address(&alith_secret_key())
    }

    /// Convert `H160` to `AccountId32`.
    pub fn account_id_from(address: H160) -> AccountId32 {
        <Runtime as pallet_evm::Config>::AddressMapping::into_account_id(address)
    }

    /// Deploy an EVM contract with code via ALICE as origin.
    pub fn deploy_evm_contract(code: &str) -> H160 {
        assert_ok!(EVM::create2(
            RuntimeOrigin::root(),
            alith(),
            hex::decode(code).expect("invalid code hex"),
            H256::zero(),
            U256::zero(),
            1_000_000,
            U256::from(DefaultBaseFeePerGas::get()),
            None,
            None,
            vec![],
        ));
        match System::events()
            .iter()
            .last()
            .expect("no event found")
            .event
        {
            RuntimeEvent::EVM(pallet_evm::Event::Created { address }) => address,
            _ => panic!("Deploy failed."),
        }
    }

    /// Deploy a WASM contract via ALICE as origin. (The code is in `../ink-contracts/`.)
    /// Assumption: Contract constructor is called "new" and take no arguments
    pub fn deploy_wasm_contract(name: &str) -> AccountId32 {
        let (address, _) = astar_test_utils::deploy_wasm_contract::<Runtime>(
            name,
            ALICE,
            0,
            Weight::from_parts(10_000_000_000, 1024 * 1024),
            None,
            hex::decode("9bae9d5e").expect("invalid data hex"),
        );

        // On instantiation, the contract got existential deposit.
        assert_eq!(Balances::free_balance(&address), ExistentialDeposit::get(),);
        address
    }

    /// Call a wasm smart contract method
    pub fn call_wasm_contract_method<V: Decode>(
        origin: AccountId,
        contract_id: AccountId,
        data: Vec<u8>,
    ) -> V {
        let (value, _, _) = astar_test_utils::call_wasm_contract_method::<Runtime, V>(
            origin,
            contract_id,
            0,
            Weight::from_parts(10_000_000_000, 1024 * 1024),
            None,
            data,
            false,
        );
        value
    }

    /// Build the signature payload for given native account and eth private key
    fn get_evm_signature(who: &AccountId32, secret: &libsecp256k1::SecretKey) -> [u8; 65] {
        // sign the payload
        UnifiedAccounts::eth_sign_prehash(&UnifiedAccounts::build_signing_payload(who), secret)
    }

    /// Create the mappings for the accounts
    pub fn connect_accounts(who: &AccountId32, secret: &libsecp256k1::SecretKey) {
        assert_ok!(UnifiedAccounts::claim_evm_address(
            RuntimeOrigin::signed(who.clone()),
            UnifiedAccounts::eth_address(secret),
            get_evm_signature(who, secret)
        ));
    }
}

#[cfg(feature = "shiden")]
pub use shiden::*;
#[cfg(feature = "shiden")]
mod shiden {
    pub use shiden_runtime::*;

    /// 1 SDN.
    pub const UNIT: Balance = SDN;
}

#[cfg(feature = "astar")]
pub use astar::*;
#[cfg(feature = "astar")]
mod astar {
    pub use astar_runtime::*;

    /// 1 ASTR.
    pub const UNIT: Balance = ASTR;
}

pub const ALICE: AccountId32 = AccountId32::new([1_u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([2_u8; 32]);
pub const CAT: AccountId32 = AccountId32::new([3_u8; 32]);

pub const INITIAL_AMOUNT: u128 = 100_000 * UNIT;

pub type SystemError = frame_system::Error<Runtime>;
pub use pallet_balances::Call as BalancesCall;
pub use pallet_dapp_staking_v3 as DappStakingCall;
pub use pallet_dapps_staking as DappsStakingCall;
pub use pallet_proxy::Event as ProxyEvent;
pub use pallet_utility::{Call as UtilityCall, Event as UtilityEvent};

pub struct ExtBuilder {
    balances: Vec<(AccountId32, Balance)>,
}

impl Default for ExtBuilder {
    fn default() -> Self {
        Self { balances: vec![] }
    }
}

impl ExtBuilder {
    pub fn balances(mut self, balances: Vec<(AccountId32, Balance)>) -> Self {
        self.balances = balances;
        self
    }

    pub fn build(self) -> sp_io::TestExternalities {
        let mut t = frame_system::GenesisConfig::default()
            .build_storage::<Runtime>()
            .unwrap();

        pallet_balances::GenesisConfig::<Runtime> {
            balances: self.balances,
        }
        .assimilate_storage(&mut t)
        .unwrap();

        // Needed to trigger initial inflation config setting.
        <pallet_inflation::GenesisConfig as GenesisBuild<Runtime>>::assimilate_storage(
            &pallet_inflation::GenesisConfig::default(),
            &mut t,
        )
        .unwrap();

        let mut ext = sp_io::TestExternalities::new(t);
        ext.execute_with(|| {
            System::set_block_number(1);

            let era_length = <Runtime as pallet_dapp_staking_v3::Config>::CycleConfiguration::blocks_per_era();
            let voting_period_length_in_eras =
            <Runtime as pallet_dapp_staking_v3::Config>::CycleConfiguration::eras_per_voting_subperiod();

            pallet_dapp_staking_v3::ActiveProtocolState::<Runtime>::put(pallet_dapp_staking_v3::ProtocolState {
                era: 1,
                next_era_start: era_length.saturating_mul(voting_period_length_in_eras.into()) + 1,
                period_info: pallet_dapp_staking_v3::PeriodInfo {
                    number: 1,
                    subperiod: pallet_dapp_staking_v3::Subperiod::Voting,
                    next_subperiod_start_era: 2,
                },
                maintenance: false,
            });
            pallet_dapp_staking_v3::Safeguard::<Runtime>::put(false);
        });
        ext
    }
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    ExtBuilder::default()
        .balances(vec![
            (ALICE, INITIAL_AMOUNT),
            (BOB, INITIAL_AMOUNT),
            (CAT, INITIAL_AMOUNT),
        ])
        .build()
}

// Block time: 12 seconds.
pub const BLOCK_TIME: u64 = 12_000;

pub fn run_to_block(n: BlockNumber) {
    while System::block_number() < n {
        let block_number = System::block_number();
        TransactionPayment::on_finalize(block_number);
        DappStaking::on_finalize(block_number);
        Authorship::on_finalize(block_number);
        Session::on_finalize(block_number);
        AuraExt::on_finalize(block_number);
        PolkadotXcm::on_finalize(block_number);
        Ethereum::on_finalize(block_number);
        CollatorSelection::on_finalize(block_number);
        DynamicEvmBaseFee::on_finalize(block_number);
        Inflation::on_finalize(block_number);

        System::set_block_number(block_number + 1);
        let block_number = System::block_number();

        Inflation::on_initialize(block_number);
        Timestamp::set_timestamp(block_number as u64 * BLOCK_TIME);
        TransactionPayment::on_initialize(block_number);
        DappStaking::on_initialize(block_number);
        Authorship::on_initialize(block_number);
        Aura::on_initialize(block_number);
        AuraExt::on_initialize(block_number);
        CollatorSelection::on_initialize(block_number);
        Ethereum::on_initialize(block_number);
        DynamicEvmBaseFee::on_initialize(block_number);
        #[cfg(any(feature = "shibuya", feature = "shiden"))]
        RandomnessCollectiveFlip::on_initialize(block_number);

        XcmpQueue::on_idle(block_number, Weight::MAX);
        DmpQueue::on_idle(block_number, Weight::MAX);
        Contracts::on_idle(block_number, Weight::MAX);
    }
}

pub fn run_for_blocks(n: BlockNumber) {
    run_to_block(System::block_number() + n)
}

fn last_events(n: usize) -> Vec<RuntimeEvent> {
    frame_system::Pallet::<Runtime>::events()
        .into_iter()
        .rev()
        .take(n)
        .rev()
        .map(|e| e.event)
        .collect()
}

pub fn expect_events(e: Vec<RuntimeEvent>) {
    assert_eq!(last_events(e.len()), e);
}

/// Initialize `env_logger` for tests. It will enable logging like `DEBUG`
/// and `TRACE` in runtime.
#[allow(dead_code)]
pub fn init_env_logger() {
    let _ = env_logger::builder().is_test(true).try_init();
}
