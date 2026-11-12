use notepad_matrix::hashes::{HashAlgorithm, selection_digest};

#[test]
fn selection_hashes_follow_source_lowercase_utf8_nul_and_empty_selection_contracts() {
    for (algorithm, expected) in [
        (HashAlgorithm::Md5, "900150983cd24fb0d6963f7d28e17f72"),
        (
            HashAlgorithm::Sha1,
            "a9993e364706816aba3e25717850c26c9cd0d89d",
        ),
        (
            HashAlgorithm::Sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            HashAlgorithm::Sha512,
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
        ),
    ] {
        assert_eq!(
            selection_digest("abc", algorithm).as_deref(),
            Some(expected)
        );
        assert_eq!(
            selection_digest("abc\0ignored", algorithm).as_deref(),
            Some(expected)
        );
        assert_eq!(selection_digest("", algorithm), None);
    }
    assert_eq!(
        selection_digest("\0abc", HashAlgorithm::Sha256).as_deref(),
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
    assert_eq!(
        selection_digest("中😀\r\n", HashAlgorithm::Sha256).as_deref(),
        Some("6445315ceca032100c2680a64658a2e15e4239d31dbc78f0ac2de1d59018ac2f")
    );
}
