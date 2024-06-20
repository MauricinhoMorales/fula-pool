#![cfg_attr(not(feature = "std"), no_std)]

use codec::EncodeLike;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::Defensive;
use frame_support::traits::Len;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;
use sp_std::fmt::Debug;
use core::cmp;
use frame_support::pallet_prelude::Weight;


pub use pallet::*;

pub trait WeightInfo {
    fn create() -> Weight;
    fn leave_pool() -> Weight;
    fn join() -> Weight;
    fn cancel_join() -> Weight;
    fn vote() -> Weight;
}
impl WeightInfo for () {
    fn create() -> Weight {
        Weight::from_all(10_000u64)
    }
    fn leave_pool() -> Weight {
        Weight::from_all(10_000u64)
    }
    fn join() -> Weight {
        Weight::from_all(10_000u64)
    }
    fn cancel_join() -> Weight {
        Weight::from_all(10_000u64)
    }
    fn vote() -> Weight {
        Weight::from_all(10_000u64)
    }
}

pub trait PoolInterface {
    type AccountId;
    type PoolId: Copy + TypeInfo + Debug + Eq + EncodeLike + Encode + Decode;
    fn is_member(account: Self::AccountId, pool: Self::PoolId) -> bool;
}

use frame_support::{dispatch::DispatchResult, ensure, traits::Get, BoundedVec};
use sp_std::prelude::*;

/// Type used for a unique identifier of each pool.
pub type PoolId = u32;

pub type BoundedStringOf<T> = BoundedVec<u8, <T as Config>::StringLimit>;

/// Pool
/// TODO: we we need an actual list of users in each pool? If so - we'll need to rething the storage
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct Pool<T: Config> {
    /// Pool name, bounded by Config::StringLimit
    pub name: BoundedVec<u8, T::StringLimit>,
    /// Optional owner, there is no pool owner when a pool is created by the system.
    pub owner: Option<T::AccountId>,
    /// Optional parent, only set when a pool has been created by the system. Unset when the pool
    /// reaches at least 3 members.
    pub parent: Option<PoolId>,
    /// The current pool participants.
    pub participants: BoundedVec<T::AccountId, T::MaxPoolParticipants>,
    /// Number of outstanding join requests.
    pub request_number: u8,
    // Region of the pool
    pub region: BoundedVec<u8, T::StringLimit>,
}

impl<T: Config> Pool<T> {
    pub fn is_full(&self) -> bool {
        self.participants.len() + self.request_number as usize
            == T::MaxPoolParticipants::get() as usize
    }
}

/// User data for pool users. Created if a user has been previously unknown by the pool system, in
/// case of a new user trying to create or join a pool.
#[derive(Clone, Encode, Decode, Default, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct User<BoundedString> {
    /// Optional PoolId, signifies membership in a pool.
    pub pool_id: Option<PoolId>,
    /// Signifies whether or not a user has a pending join request to a given pool. If this is set -
    /// the `pool_id` should be `None`.
    pub request_pool_id: Option<PoolId>,
    /// libp2p peerID validated on the client-side.
    pub peer_id: BoundedString,
}

impl<BoundedString> User<BoundedString> {
    /// Signifies whether or not a user can create or join a pool.
    pub(crate) fn is_free(&self) -> bool {
        self.pool_id.is_none() && self.request_pool_id.is_none()
    }
}

/// An enum that represents a vote result.
pub(crate) enum VoteResult {
    /// Majority voted for.
    Accepted,
    /// Majority voted against.
    Denied,
    /// Not conclusive yet.
    Inconclusive,
}

/// The current implementation of `PoolJoinRequest` only cares about positive votes and keeps track
/// of everyone that voted.
/// TODO: we might have to cover corner-cases, such as:
/// 1. When a user voted for somebody and left (possibly not the worst case)
/// 2. When a user left from the pool without voting, we can only recalculate this when another user
/// votes
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct PoolRequest<T: Config> {
    /// Prevents a user to vote twice on the same `PoolJoinRequest`.
    pub voted: BoundedVec<T::AccountId, T::MaxPoolParticipants>,
    /// Currently we only calculate positive votes to avoid having to iterate through voters map.
    /// We can easily calculate negative votes by taking the `voted` length and subtracting
    /// `positivte_votes` from it.
    pub positive_votes: u16,
    /// libp2p peerID validated on the client-side. A pre-requisite for voting
    pub peer_id: BoundedVec<u8, T::StringLimit>,
}

impl<T: Config> Default for PoolRequest<T> {
    fn default() -> Self {
        PoolRequest {
            positive_votes: Default::default(),
            voted: Default::default(),
            peer_id: Default::default(),
        }
    }
}

impl<T: Config> Default for Pool<T> {
    fn default() -> Self {
        Pool {
            name: BoundedVec::default(), 
            owner: None, 
            parent: None, 
            participants: BoundedVec::default(), 
            request_number: 0, 
            region: BoundedVec::default(),  
        }
    }
}

impl<T: Config> PoolRequest<T> {
    /// A method that checks whether or not a user has been accepted to a pool.
    pub(crate) fn check_votes(&self, num_participants: u16, pool_id: PoolId) -> VoteResult { // Include pool_id
        let pool = Pallet::<T>::pool(pool_id).unwrap_or_default(); // Fetch the pool

         // Short-circuit approval if the pool owner voted yes
        for voter in &self.voted {
            if let Some(ref owner) = pool.owner {
                if *voter == *owner && self.positive_votes >= 1 {
                    return VoteResult::Accepted;
                }
            }            
        }

        // Continue with existing logic
        let min_votes_required = cmp::min(num_participants / 3, 8);

        // More than half of the participants voted against this user.
        if self.voted.len() as u16 - self.positive_votes > num_participants / 2 {
            return VoteResult::Denied;
        }

        if self.positive_votes > min_votes_required {
            return VoteResult::Accepted;
        }

        VoteResult::Inconclusive
    }
}

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

// TODO: Implement benchmarks for proper weight calculation
#[frame_support::pallet]
pub mod pallet {
    use crate::*;
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_runtime::BoundedVec;

    #[pallet::pallet]
    #[pallet::generate_store(pub (super) trait Store)]
    pub struct Pallet<T>(_);

    #[pallet::config]
    /// The module configuration trait.
    pub trait Config: frame_system::Config {
        /// The overarching event type.
        type RuntimeEvent: From<Event<Self>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>
            + TryInto<Event<Self>>;

        /// The maximum length of a name or symbol stored on-chain. See if this can be limited to
        /// `u8::MAX`.
        #[pallet::constant]
        type StringLimit: Get<u32>;

        /// The maximum number of pool participants. We are aiming at `u8::MAX`.
        #[pallet::constant]
        type MaxPoolParticipants: Get<u32>;

        type WeightInfo: WeightInfo;
    }

    /// An incremental value reflecting all pools created so far.
    #[pallet::storage]
    pub type LastPoolId<T: Config> = StorageValue<_, PoolId, ValueQuery>;

    /// Maximum number of pools that can exist. If `None`, then an unbounded number of
    /// pools can exist.
    #[pallet::storage]
    pub type MaxPools<T: Config> = StorageValue<_, PoolId, OptionQuery>;

    /// Pools storage
    #[pallet::storage]
    #[pallet::getter(fn pool)]
    pub type Pools<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, Pool<T>, OptionQuery>;

    /// PoolRequests storage
    #[pallet::storage]
    #[pallet::getter(fn request)]
    pub type PoolRequests<T: Config> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        PoolId,
        Blake2_128Concat,
        T::AccountId,
        PoolRequest<T>,
        OptionQuery,
    >;

    /// Users storage, useful in case a user wants to leave or join a pool.
    #[pallet::storage]
    #[pallet::getter(fn user)]
    pub type Users<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, User<BoundedStringOf<T>>>;

    /// The events of this pallet.
    #[pallet::event]
    #[pallet::generate_deposit(pub (super) fn deposit_event)]
    pub enum Event<T: Config> {
        /// A pool has been created.
        PoolCreated {
            owner: Option<T::AccountId>,
            pool_id: PoolId,
        },

        /// A user requested to join a pool.
        JoinRequested {
            account: T::AccountId,
            pool_id: PoolId,
        },

        /// A user has withdrawn their request to join a pool.
        RequestWithdrawn {
            account: T::AccountId,
            pool_id: PoolId,
        },

        // Results of the voting proccess
        VotingResult {
            account: T::AccountId,
            pool_id: PoolId,
            result: Vec<u8>,
        },
        /// Pool's capacity has been reached,
        CapacityReached { pool_id: PoolId },

        /// Pool participant left.
        ParticipantLeft {
            account: T::AccountId,
            pool_id: PoolId,
        },
    }

    #[pallet::error]
    #[cfg_attr(test, derive(PartialEq))]
    pub enum Error<T> {
        /// User is already attached to a pool or has a pending join request.
        UserBusy,
        /// Maximum pool number has been reached.
        MaxPools,
        /// The pool name supplied was too long.
        NameTooLong,
        /// The pool does not exist.
        PoolDoesNotExist,
        /// The pool join request does not exist.
        RequestDoesNotExist,
        /// The pool is at max capacity.
        CapacityReached,
        /// The user does not exist.
        UserDoesNotExist,
        /// Access denied due to invalid data, e.g. user is trying to leave the pool that it does
        /// not belong to or vote without rights.
        AccessDenied,
        /// Internal error.
        InternalError,
        /// The user has already voted.
        /// TODO: might be considered slashable behaviour as it wastes resources.
        AlreadyVoted,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Creates a new pool. `peer_id` is a libp2p peerID validated on the client-side.
        ///
        /// TODO: Deposit; check the current pool number. Currently we check the PoolId to retrieve
        /// the pool number, but if we want to delete empty pools - then we need to retrieve the
        /// actual pool number from storage, for which a CountedMap should be used.
        #[pallet::weight(T::WeightInfo::create())]
        #[pallet::call_index(0)]
        pub fn create(
            origin: OriginFor<T>,
            name: Vec<u8>,
            region: Vec<u8>,
            peer_id: BoundedVec<u8, T::StringLimit>,
        ) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let mut user = Self::get_or_create_user(&owner);

            ensure!(user.is_free(), Error::<T>::UserBusy);

            if let Some(max_pools) = MaxPools::<T>::get() {
                ensure!(max_pools > LastPoolId::<T>::get(), Error::<T>::MaxPools);
            }

            let pool_id = LastPoolId::<T>::mutate(|id| {
                *id += 1;
                *id
            });

            let bounded_name: BoundedVec<u8, T::StringLimit> = name
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::NameTooLong)?;

            let bounded_region: BoundedVec<u8, T::StringLimit> = region
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::NameTooLong)?;

            let mut bounded_participants =
                BoundedVec::<T::AccountId, T::MaxPoolParticipants>::default();

            ensure!(
                bounded_participants.try_push(owner.clone()).is_ok(),
                Error::<T>::CapacityReached
            );

            let pool = Pool {
                name: bounded_name,
                region: bounded_region,
                owner: Some(owner.clone()),
                parent: None,
                participants: bounded_participants,
                request_number: 0,
            };
            Pools::<T>::insert(pool_id.clone(), pool);
            user.pool_id = Some(pool_id.clone());
            user.peer_id = peer_id;
            Users::<T>::set(&owner, Some(user));

            Self::deposit_event(Event::<T>::PoolCreated {
                pool_id,
                owner: Some(owner),
            });

            Ok(())
        }

        /// Allows for the user to leave a pool.
        #[pallet::weight(T::WeightInfo::leave_pool())]
        #[pallet::call_index(1)]
        pub fn leave_pool(
            origin: OriginFor<T>, 
            pool_id: PoolId,
            target_account: Option<T::AccountId>, // Optional target account parameter
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            // Retrieve the pool to check if the caller is the owner
            let pool = Pools::<T>::get(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;

            // Determine the account to be removed from the pool
            let account_to_remove = match target_account {
                Some(account) if Some(&caller) == pool.owner.as_ref() || caller == account => {
                    // If the caller is the owner, they can remove any account
                    // If the caller is the target_account themselves, they can remove themselves
                    account
                },
                None => {
                    // If no target account is specified and caller is not the owner, it defaults to the caller
                    ensure!(pool.owner.is_none() || pool.owner == Some(caller.clone()), Error::<T>::AccessDenied);
                    caller
                },
                _ => return Err(Error::<T>::AccessDenied.into()), // Deny if not owner and trying to remove another account
            };

            let mut user = Users::<T>::get(&account_to_remove).ok_or(Error::<T>::UserDoesNotExist)?;
            ensure!(
                user.pool_id.is_some() && pool_id == user.pool_id.unwrap(),
                Error::<T>::AccessDenied
            );

            let mut pool: Pool<T> = Self::pool(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;
            let mut participants = pool.participants.clone();

            match participants.binary_search(&account_to_remove) {
                Ok(index) => {
                    participants.remove(index);
                    pool.participants = participants;
                    Pools::<T>::set(&pool_id, Some(pool));

                    user.pool_id = None;
                    Users::<T>::set(&account_to_remove, Some(user));

                    Self::deposit_event(Event::<T>::ParticipantLeft { pool_id, account: account_to_remove });
                    Ok(())
                }
                // This should never happen, but if it does - what do we do? One option is to
                // deposit an error event. The problem here is that a user will be permanently stuck
                // in an inconsistent state due to the fact that they have a pool_id in their
                // profile, but they are not actually a member of a pool. This is a defensive check.
                Err(_) => {
                    frame_support::defensive!(
                        "a user is not a participant of the pool they are assigned to"
                    );
                    Err(Error::<T>::InternalError.into())
                }
            }
        }

        /// Open a `PoolRequest` to join the pool.
        #[pallet::weight(T::WeightInfo::join())]
        #[pallet::call_index(2)]
        pub fn join(
            origin: OriginFor<T>,
            pool_id: PoolId,
            peer_id: BoundedVec<u8, T::StringLimit>,
        ) -> DispatchResult {
            let account = ensure_signed(origin)?;
            let mut pool = Self::pool(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;

            ensure!(!pool.is_full(), Error::<T>::CapacityReached);

            let mut user = Self::get_or_create_user(&account);

            ensure!(user.is_free(), Error::<T>::UserBusy);

            user.request_pool_id = Some(pool_id);
            user.peer_id = peer_id.clone();
            Users::<T>::set(&account, Some(user));

            let mut request = PoolRequest::<T>::default();
            request.peer_id = peer_id;
            PoolRequests::<T>::insert(&pool_id, &account, request);
            pool.request_number += 1;
            Pools::<T>::set(&pool_id, Some(pool));

            Self::deposit_event(Event::<T>::JoinRequested { pool_id, account });
            Ok(())
        }

        /// Cancel a `PoolRequest`, useful if a user decides to join another pool or they are stuck in
        /// the voting queue for too long.
        #[pallet::weight(T::WeightInfo::cancel_join())]
        #[pallet::call_index(3)]
        pub fn cancel_join(
            origin: OriginFor<T>, 
            pool_id: PoolId, 
            target_account: Option<T::AccountId>, // Optional target account parameter
        ) -> DispatchResult {
            let caller = ensure_signed(origin)?;

            // Retrieve the pool to check if the caller is the owner
            let pool = Self::pool(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;

            // Determine the account whose join request is being cancelled
            let account_to_cancel = match target_account {
                Some(account) if Some(&caller) == pool.owner.as_ref() => {
                    // If the caller is the owner, they can cancel anyone's request
                    account
                },
                None => {
                    // If no target account is specified, it defaults to the caller
                    // This maintains backward compatibility for users cancelling their own requests
                    caller.clone()
                },
                _ => return Err(Error::<T>::AccessDenied.into()), // Deny if not owner and trying to cancel for another account
            };
            
            Self::request(&pool_id, &account_to_cancel).ok_or(Error::<T>::RequestDoesNotExist)?;
            let mut user = Self::user(&account_to_cancel).ok_or(Error::<T>::UserDoesNotExist)?;
            let pool = Self::pool(&pool_id).ok_or(Error::<T>::PoolDoesNotExist)?;

            user.request_pool_id = None;
            Users::<T>::set(&account_to_cancel, Some(user));

            PoolRequests::<T>::remove(&pool_id, &account_to_cancel);

            Self::remove_pool_request(&account_to_cancel, pool_id, pool);

            Self::deposit_event(Event::<T>::RequestWithdrawn { 
                pool_id, 
                account: account_to_cancel, 
            });
            
            Ok(())
        }

        /// Vote for a `PoolRequest`. If `positive` is set to `false` - that's voting against.
        /// This method also calculates votes each time it's called and takes action once the result
        /// is conclusive.
        /// TODO: Currently does not cover pool overflow scenario and simply fails then.
        #[pallet::weight(T::WeightInfo::vote())]
        #[pallet::call_index(4)]
        pub fn vote(
            origin: OriginFor<T>,
            pool_id: PoolId,
            account: T::AccountId,
            positive: bool,
            peer_id: BoundedVec<u8, T::StringLimit>,
        ) -> DispatchResult {
            let voter = ensure_signed(origin)?;
            let voter_user = Self::get_user(&voter)?;

            let mut request =
                Self::request(&pool_id, &account).ok_or(Error::<T>::RequestDoesNotExist)?;

            ensure!(
                voter_user.pool_id.is_some() && voter_user.pool_id.unwrap() == pool_id,
                Error::<T>::AccessDenied
            );

            ensure!(
                request.peer_id.to_vec() == peer_id.to_vec(),
                Error::<T>::AccessDenied
            );

            let mut voted = request.voted.clone();

            match voted.binary_search(&voter) {
                Ok(_) => Err(Error::<T>::AlreadyVoted.into()),

                Err(index) => {
                    // This should never fail.
                    voted
                        .try_insert(index, voter.clone())
                        .map_err(|_| Error::<T>::InternalError)
                        .defensive()?;

                    // Increment votes if positive, we do all that here to be able to calculate the
                    // votes as we need to do it on every vote.
                    if positive {
                        request.positive_votes += 1;
                    }
                    request.voted = voted;

                    // This should never fail.
                    let pool = Self::pool(&pool_id)
                        .ok_or(Error::<T>::PoolDoesNotExist)
                        .defensive()?;

                    // TODO: to be removed when we implement copy for pools.
                    ensure!(!pool.is_full(), Error::<T>::CapacityReached);

                    let result = request.check_votes(pool.participants.len() as u16, pool_id);
                    Self::process_vote_result(&account, pool_id, pool, request, result)
                }
            }
        }
    }

    impl<T: Config> Pallet<T> {
        /// Get or create a user
        fn get_or_create_user(who: &T::AccountId) -> User<BoundedStringOf<T>> {
            if let Some(user) = Self::user(who) {
                return user;
            }
            let user = User::default();

            Users::<T>::insert(who, user.clone());

            user
        }

        fn get_user(who: &T::AccountId) -> Result<User<BoundedStringOf<T>>, DispatchError> {
            Self::user(who).ok_or(Error::<T>::UserDoesNotExist.into())
        }

        fn remove_pool_request(who: &T::AccountId, pool_id: PoolId, mut pool: Pool<T>) {
            PoolRequests::<T>::remove(pool_id, who);
            pool.request_number -= 1;
            Pools::<T>::set(&pool_id, Some(pool));
        }

        fn process_vote_result(
            who: &T::AccountId,
            pool_id: PoolId,
            mut pool: Pool<T>,
            request: PoolRequest<T>,
            result: VoteResult,
        ) -> DispatchResult {
            match result {
                // If the user has been accepted - remove the PoolRequest, update the user to
                // link them to the pool and remove PoolRequest reference. Also add them to
                // pool participants.
                VoteResult::Accepted => {
                    // This should never fail.
                    let mut user = Self::get_user(who).defensive()?;

                    PoolRequests::<T>::remove(pool_id, who);
                    let mut participants = pool.participants.clone();
                    match participants.binary_search(who) {
                        // should never happen
                        Ok(_) => Err(Error::<T>::InternalError.into()),
                        Err(index) => {
                            participants
                                .try_insert(index, who.clone())
                                .map_err(|_| Error::<T>::InternalError)
                                .defensive()?;
                            user.pool_id = Some(pool_id.clone());
                            user.request_pool_id = None;
                            user.peer_id = request.peer_id.into();
                            Users::<T>::set(who, Some(user));

                            pool.participants = participants;
                            Self::remove_pool_request(who, pool_id, pool);
                            let result = "Accepted";

                            Self::deposit_event(Event::<T>::VotingResult {
                                pool_id,
                                account: who.clone(),
                                result: result.as_bytes().to_vec(),
                            });
                            Ok(())
                        }
                    }
                    .defensive()
                }
                // If the user has been denied access to the pool - remove the PoolRequest
                // and it's reference from the user profile.
                VoteResult::Denied => {
                    let mut user = Self::get_user(who).defensive()?;
                    user.request_pool_id = None;
                    Users::<T>::set(who, Some(user));

                    Self::remove_pool_request(who, pool_id, pool);
                    let result = "Denied";

                    Self::deposit_event(Event::<T>::VotingResult {
                        pool_id,
                        account: who.clone(),
                        result: result.as_bytes().to_vec(),
                    });

                    Ok(())
                }
                // If the vote result is inconclusive - just set the incremented vote count
                // and add the voter to the PoolRequest.
                VoteResult::Inconclusive => {
                    PoolRequests::<T>::set(&pool_id, who, Some(request));
                    let result = "Inconclusive";
                    Self::deposit_event(Event::<T>::VotingResult {
                        pool_id,
                        account: who.clone(),
                        result: result.as_bytes().to_vec(),
                    });
                    Ok(())
                }
            }
        }
    }

    impl<T: Config> PoolInterface for Pallet<T> {
        type AccountId = T::AccountId;
        type PoolId = PoolId;

        fn is_member(account: Self::AccountId, pool: Self::PoolId) -> bool {
            Self::user(&account)
                .map(|u| u.pool_id.iter().any(|&v| v == pool))
                .unwrap_or(false)
        }
    }
}
