#![cfg(feature = "runtime-benchmarks")]

use super::*;
use frame_benchmarking::{benchmarks, whitelisted_caller, impl_benchmark_test_suite};
use frame_system::{self, RawOrigin};

// Correctly importing whitelisted_caller and other necessary functions
use frame_benchmarking::account;

benchmarks! {
    create {
        let caller: T::AccountId = whitelisted_caller();
        let name = vec![0; T::StringLimit::get() as usize].try_into().expect("name is too long");
        let region = vec![0; T::StringLimit::get() as usize].try_into().expect("region is too long");
        let peer_id: BoundedVec<u8, T::StringLimit> = vec![0; 32].try_into().unwrap();
    }: _(RawOrigin::Signed(caller), name, region, peer_id)
    verify { /* verification code */ }

    leave_pool {
        let caller: T::AccountId = whitelisted_caller();
        let pool_id: PoolId = 1; // Example, generate or fetch a valid pool_id
        let target_account: T::AccountId = account("target", 0, 0);
    }: _(RawOrigin::Signed(caller), pool_id, Some(target_account))
    verify { /* verification code */ }

    join {
        let caller: T::AccountId = whitelisted_caller();
        let pool_id: PoolId = 1; // Example, generate or fetch a valid pool_id
        let peer_id: BoundedVec<u8, T::StringLimit> = vec![0; 32].try_into().unwrap();
    }: _(RawOrigin::Signed(caller), pool_id, peer_id)
    verify { /* verification code */ }

    cancel_join {
        let caller: T::AccountId = whitelisted_caller();
        let pool_id: PoolId = 1; // Example, generate or fetch a valid pool_id
        let target_account: T::AccountId = account("target", 0, 0);
    }: _(RawOrigin::Signed(caller), pool_id, Some(target_account))
    verify { /* verification code */ }

    vote {
        let caller: T::AccountId = whitelisted_caller();
        let pool_id: PoolId = 1; // Example, generate or fetch a valid pool_id
        let account_to_vote: T::AccountId = account("votee", 0, 0);
        let positive: bool = true;
        let peer_id: BoundedVec<u8, T::StringLimit> = vec![0; 32].try_into().unwrap();
    }: _(RawOrigin::Signed(caller), pool_id, account_to_vote, positive, peer_id)
    verify { /* verification code */ }
}

// Ensure the benchmark test suite is correctly implemented for your pallet.
impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
