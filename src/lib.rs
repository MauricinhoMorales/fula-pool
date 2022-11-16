use crate::pallet::Config;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::bounded::BoundedVec;
use sp_runtime::RuntimeDebug;

/// Type used for a unique identifier of each pool.
pub type PoolId = u32;

pub type BoundedVecOf<T> = BoundedVec<u8, <T as Config>::StringLimit>;

/// Pool
#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, Default, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct Pool<T: Config> {
    /// Pool name, bounded by Config::StringLimit
    pub name: BoundedVecOf<T>,
    /// Optional owner, there is no pool owner when a pool is created by the system.
    pub owner: Option<T::AccountId>,
    /// Optional parent, only set when a pool has been created by the system. Unset when the pool
    /// reaches at least 3 members.
    pub parent: Option<PoolId>,
}

#[derive(Clone, Encode, Decode, Eq, PartialEq, RuntimeDebug, Default, TypeInfo, MaxEncodedLen)]
pub struct User {
    pub pool_id: Option<PoolId>,
    pub join_requested: bool,
}

impl User {
    pub(crate) fn is_free(&self) -> bool {
        self.pool_id.is_none() && !self.join_requested
    }
}

// TODO: Implement benchmarks for proper weight calculation
#[frame_support::pallet]
pub mod pallet {
    use crate::{BoundedVecOf, Pool, PoolId, User};
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;

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

        /// The maximum length of a name or symbol stored on-chain.
        #[pallet::constant]
        type StringLimit: Get<u32>;
    }

    /// An incremental value reflecting all pools created so far.
    #[pallet::storage]
    pub type LastPoolId<T: Config> = StorageValue<_, PoolId, ValueQuery>;

    /// Maximum number of pools that can exist. If `None`, then an unbounded number of
    /// pools can exist.
    #[pallet::storage]
    pub type MaxPools<T: Config> = StorageValue<_, PoolId, OptionQuery>;

    /// Maximum number of members that may belong to a pool. If `None`, then the count of
    /// members is not bound on a per pool basis.
    #[pallet::storage]
    pub type MaxPoolMembersPerPool<T: Config> = StorageValue<_, u16, OptionQuery>;

    /// Pools storage
    #[pallet::storage]
    pub type Pools<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, Pool<T>, OptionQuery>;
    //
    // /// PoolRequests storage
    // #[pallet::storage]
    // pub type PoolRequests<T: Config> = StorageDoubleMap<
    //     _,
    //     Blake2_128Concat,
    //     PoolId,
    //     Blake2_128Concat,
    //     T::AccountId,
    //     PoolRequest<T>,
    //     OptionQuery,
    // >;

    /// Users storage, useful in case a user wants to leave or join a pool.
    #[pallet::storage]
    pub type Users<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, User>;

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
        JoinRequested { user: T::AccountId, pool_id: PoolId },

        /// A user has been accepted to the pool
        Accepted { user: T::AccountId, pool_id: PoolId },

        /// A user has been denied access to the pool.
        Denied {
            stash: T::AccountId,
            pool_id: PoolId,
        },
        /// Pool's capacity has been reached,
        CapacityReached { pool_id: PoolId },
    }

    #[pallet::error]
    #[cfg_attr(test, derive(PartialEq))]
    pub enum Error<T> {
        /// User is already attached to a pool or has a pending join request.
        UserBusy,

        /// Maximum pool number has been reached
        MaxPools,

        /// The pool name supplied was too long
        NameTooLong,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(10_000)]
        /// TODO: Deposit; check the current pool number. Currently we check the PoolId to retrieve
        /// the pool number, but if we want to delete empty pools - then we need to retrieve the
        /// actual pool number from storage, for which a CountedMap should be used.
        pub fn create(origin: OriginFor<T>, name: Vec<u8>) -> DispatchResult {
            let owner = ensure_signed(origin)?;
            let user = Self::get_user(&owner);

            ensure!(user.is_free(), Error::<T>::UserBusy);

            if let Some(max_pools) = MaxPools::<T>::get() {
                ensure!(max_pools > LastPoolId::<T>::get(), Error::<T>::MaxPools);
            }

            let pool_id = LastPoolId::<T>::mutate(|id| {
                *id += 1;
                *id
            });

            let bounded_name: BoundedVecOf<T> = name
                .clone()
                .try_into()
                .map_err(|_| Error::<T>::NameTooLong)?;

            let pool = Pool {
                name: bounded_name,
                owner: Some(owner.clone()),
                parent: None,
            };

            Pools::<T>::insert(pool_id.clone(), pool);

            Self::deposit_event(Event::<T>::PoolCreated {
                pool_id,
                owner: Some(owner),
            });

            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        /// Get or create a user
        fn get_user(who: &T::AccountId) -> User {
            if let Some(user) = Users::<T>::get(who) {
                return user;
            }
            let user = User {
                pool_id: None,
                join_requested: false,
            };

            Users::<T>::insert(who, user.clone());

            user
        }
    }
}
