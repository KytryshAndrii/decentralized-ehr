use ehr_desktop::utils::documents_label;

#[test]
fn documents_label_polish_plural() {
    let labels: Vec<String> = [0, 1, 2, 4, 5, 12, 14, 21, 22, 112]
        .map(documents_label)
        .to_vec();
    assert_eq!(
        labels,
        [
            "0 dokumentów",
            "1 dokument",
            "2 dokumenty",
            "4 dokumenty",
            "5 dokumentów",
            "12 dokumentów",
            "14 dokumentów",
            "21 dokumentów",
            "22 dokumenty",
            "112 dokumentów"
        ]
    );
}
