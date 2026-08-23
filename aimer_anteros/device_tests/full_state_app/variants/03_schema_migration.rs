use aimer_wasm_guest::anteros::Version;
use crate::guest_support::{FullStateProgram, GuestVariant, migrate_v1_to_v2};
pub struct SchemaMigrationVariant;
impl GuestVariant for SchemaMigrationVariant {
    const SCHEMA_VERSION: Version = Version::new(2, 0);
    const STATE_TAG: u8 = 2;
    const CALLBACK_STEP: u8 = 1;
    const HEADER: &'static str = "FULL STATE / SCHEMA V2 MIGRATED";
    const BUTTON: &'static str = "increment +1 (schema v2)";
    const TRAP_ON_BUILD: bool = false;
    fn migrate(previous: Version, payload: &[u8]) -> Option<Vec<u8>> { migrate_v1_to_v2(previous, payload, Self::STATE_TAG) }
}
pub type FullStateGuest = FullStateProgram<SchemaMigrationVariant>;
