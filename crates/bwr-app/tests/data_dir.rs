//! bwr-core and bwr-plugins each resolve the data folder (they don't depend on
//! each other); both must land in the same place, and on macOS in
//! ~/Library/Application Support/BigWaveRadio.

#[test]
fn core_and_plugins_agree_on_the_data_dir() {
    // Explicit LOCALAPPDATA (always set on Windows; the test hook elsewhere).
    std::env::set_var("LOCALAPPDATA", "/tmp/bwr-data-dir-test");
    assert_eq!(bwr_core::paths::data_dir(), std::path::PathBuf::from("/tmp/bwr-data-dir-test/BigWaveRadio"));
    assert_eq!(bwr_core::paths::data_dir(), bwr_plugins::app_data_dir());
    assert_eq!(bwr_plugins::default_plugins_dir(), bwr_core::paths::data_dir().join("plugins"));

    // Platform default when it's unset.
    std::env::remove_var("LOCALAPPDATA");
    assert_eq!(bwr_core::paths::data_dir(), bwr_plugins::app_data_dir());
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(
            bwr_core::paths::data_dir(),
            std::path::PathBuf::from(home).join("Library/Application Support/BigWaveRadio")
        );
    }
    #[cfg(not(windows))]
    assert!(bwr_core::paths::data_dir().is_absolute(), "never falls back to ./ when HOME is set");
}
