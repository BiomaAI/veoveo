use veoveo_types::HttpsUrl;

#[test]
fn https_urls_preserve_network_components_without_resource_query_rules() {
    for wire in [
        "https://data.example.test/",
        "https://data.example.test:8443/measurements.csv?part=2&part=1",
        "https://data.example.test/a%2Fb/%C3%A9.csv?sig=a%2Fb%2B%3D&empty=&flag",
        "https://data.example.test/file.csv?literal=one+two&escaped=one%20two",
        "https://127.0.0.1/input.csv",
        "https://[::1]:8443/input.csv",
    ] {
        let value = HttpsUrl::parse(wire).unwrap();
        assert_eq!(value.as_str(), wire);
        assert_eq!(serde_json::to_value(&value).unwrap(), wire);
        assert_eq!(
            serde_json::from_str::<HttpsUrl>(&serde_json::to_string(&value).unwrap()).unwrap(),
            value
        );
    }
}

#[test]
fn https_urls_reject_ambiguous_or_unadmitted_spelling_without_echoing_input() {
    for wire in [
        "file:///secret.csv",
        "http://data.example.test/input.csv",
        "//data.example.test/file.csv",
        "https://user:secret@data.example.test/input.csv",
        "https://data.example.test/input.csv#secret",
        "https://data.example.test",
        "HTTPS://data.example.test/input.csv",
        "https://DATA.example.test/input.csv",
        "https://data.example.test:443/input.csv",
        "https://data.example.test/a/../input.csv",
        "https://data.example.test/input.csv?sig=%ZZ",
        "https://data.example.test/é.csv",
        "https://data.example.test\\input.csv",
        " https://data.example.test/input.csv",
        "https://data.example.test/input.csv\n",
        "https://data.example.test:99999/input.csv",
    ] {
        let error = HttpsUrl::parse(wire).unwrap_err().to_string();
        assert!(!error.contains(wire));
        assert!(!error.contains("secret"));
        assert!(serde_json::from_value::<HttpsUrl>(serde_json::json!(wire)).is_err());
    }
    let signed = HttpsUrl::parse("https://data.example.test/input.csv?token=secret").unwrap();
    assert_eq!(format!("{signed:?}"), "HttpsUrl(\"[redacted]\")");
}
