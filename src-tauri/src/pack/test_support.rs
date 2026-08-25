pub(crate) fn valid_manifest_json() -> Vec<u8> {
    br#"{
      "schema_version": 1,
      "id": "keyforge-mechanical",
      "name": "KeyForge Mechanical",
      "pack_version": "1.0.0",
      "sounds": {
        "normal": ["sounds/normal-01.wav", "sounds/normal-02.wav"],
        "space": ["sounds/space-01.wav"],
        "enter": ["sounds/enter-01.wav"],
        "backspace": ["sounds/backspace-01.wav"],
        "modifier": ["sounds/modifier-01.wav"]
      }
    }"#
    .to_vec()
}
