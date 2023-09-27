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

//! Dapps staking FRAME Pallet.

use super::*;
use frame_support::{
    pallet_prelude::*,
    traits::{Currency, Get, LockIdentifier, LockableCurrency, ReservableCurrency},
    weights::Weight,
};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::StaticLookup;
use sp_std::convert::From;

const _NOMINATION_POOL_STAKING_ID: LockIdentifier = *b"np_stake";

#[frame_support::pallet]
#[allow(clippy::module_inception)]
pub mod pallet {
    use super::*;
    use sp_std::vec;
    use xcm::opaque::lts::{OriginKind::SovereignAccount, WeightLimit};
    use xcm::v3::{
        Instruction::{BuyExecution, Transact, WithdrawAsset},
        Junctions::Here,
        MultiLocation, Xcm,
    };

    /// The balance type of this pallet.
    pub type BalanceOf<T> =
        <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[derive(Encode, Decode, RuntimeDebug)]
    pub enum NominationPoolsCall<T: Config> {
        #[codec(index = 6)] // same to call index
        Create(
            #[codec(compact)] BalanceOf<T>,
            <T::Lookup as StaticLookup>::Source,
            <T::Lookup as StaticLookup>::Source,
            <T::Lookup as StaticLookup>::Source,
        ),
    }

    #[derive(Encode, Decode, RuntimeDebug)]
    pub enum RelayChainCall<T: Config> {
        // https://github.com/paritytech/polkadot/blob/7a19bf09147605f185421a51ec254c51d2c7d060/runtime/polkadot/src/lib.rs#L1414
        #[codec(index = 39)]
        NominationPools(NominationPoolsCall<T>),
    }

    #[pallet::pallet]
    #[pallet::generate_store(pub(crate) trait Store)]
    pub struct Pallet<T>(PhantomData<T>);

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_xcm::Config {
        /// The staking balance.
        type Currency: LockableCurrency<Self::AccountId, Moment = Self::BlockNumber>
            + ReservableCurrency<Self::AccountId>;

        /// Describes smart contract in the context required by dapps staking.
        type SmartContract: Default + Parameter + Member + MaxEncodedLen;

        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// Weight information for extrinsics in this pallet.
        type WeightInfo: WeightInfo;
    }

    /// Denotes whether pallet is disabled (in maintenance mode) or not
    #[pallet::storage]
    #[pallet::whitelist_storage]
    #[pallet::getter(fn pallet_disabled)]
    pub type PalletDisabled<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Account has bonded and staked funds on a smart contract.
        BondAndStake(T::AccountId, T::SmartContract),
        DebugNominationPool(xcm::DoubleEncoded<Call<T>>),
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Disabled
        Disabled,
        /// Failed to send XCM transaction
        FailedXcmTransaction,
        /// asdf
        FailedToConvertBalance,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Lock up and stake balance of the origin account.
        ///
        /// `value` must be more than the `minimum_balance` specified by `MinimumStakingAmount`
        /// unless account already has bonded value equal or more than 'minimum_balance'.
        ///
        /// The dispatch origin for this call must be _Signed_ by the staker's account.
        #[pallet::call_index(1)]
        #[pallet::weight(<T as pallet::pallet::Config>::WeightInfo::create_nomination_pool())]
        pub fn create_nomination_pool(
            origin: OriginFor<T>,
            contract_id: T::SmartContract,
            value: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            Self::ensure_pallet_enabled()?;

            let staker: <T as frame_system::Config>::AccountId = ensure_signed(origin)?;

            let staker_multi_address = T::Lookup::unlookup(staker.clone());

            let create_nomination_pool: NominationPoolsCall<T> = NominationPoolsCall::Create(
                value,
                staker_multi_address.clone(),
                staker_multi_address.clone(),
                staker_multi_address.clone(),
            );

            let encoded_call = create_nomination_pool.encode();

            Self::deposit_event(Event::<T>::DebugNominationPool(encoded_call.into()));

            // TODO add refund surplus on error. https://paritytech.github.io/xcm-docs/journey/fees/index.html#refundsurplus

            // let withdraw = 501_000_000_000u128;
            let value: u128 = value
                .try_into()
                .map_err(|_| Error::<T>::FailedToConvertBalance)?;
            let messages = Xcm(vec![
                WithdrawAsset((Here, value).into()),
                BuyExecution {
                    fees: (Here, 1_000_000_000u128).into(),
                    weight_limit: WeightLimit::Unlimited,
                },
                // Transact {
                //     origin_kind: SovereignAccount,
                //     require_weight_at_most: Weight::from_parts(10_000_000_000u64, 1024 * 1024),
                //     call: create_nomination_pool.encode().into(),
                // },
            ]);

            match pallet_xcm::Pallet::<T>::send_xcm(Here, MultiLocation::parent(), messages) {
                Ok(_) => {
                    Self::deposit_event(Event::<T>::BondAndStake(staker, contract_id));
                    Ok(().into())
                }
                Err(_err) => Err(Error::<T>::FailedXcmTransaction.into()),
            }
        }
    }
}

impl<T: Config> Pallet<T> {
    /// `Err` if pallet disabled for maintenance, `Ok` otherwise
    pub fn ensure_pallet_enabled() -> Result<(), Error<T>> {
        if PalletDisabled::<T>::get() {
            Err(Error::<T>::Disabled)
        } else {
            Ok(())
        }
    }
}
