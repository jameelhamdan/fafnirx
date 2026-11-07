use super::*;

#[test]
fn a_folder_that_can_only_be_read_says_so() {
    let folder = SyncedStatus { last_cycle: 1_000, files: 3, read_only: true, ..Default::default() };
    assert_eq!(folder.summary(1_030), "in step, read only, 3 files, synced 30s ago");
    let own = SyncedStatus { read_only: false, ..folder };
    assert_eq!(own.summary(1_030), "in step, 3 files, synced 30s ago");
}
