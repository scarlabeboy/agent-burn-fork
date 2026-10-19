// Gate M6D transport fixture only; this is not a production observation schema.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyntheticSnapshot {
    schema_version: u8,
    provenance: &'static str,
    sources: [SyntheticSource; 2],
}

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum SourceId {
    Windows,
    SelectedWsl,
}

#[derive(serde::Serialize)]
struct SyntheticSource {
    id: SourceId,
    observations: UnavailableObservations,
}

// Serde serializes unit fields as JSON null. Measurements cannot be represented here.
#[derive(serde::Serialize)]
struct UnavailableObservations {
    usage: (),
    sessions: (),
    models: (),
    cost: (),
    quota: (),
}

const UNAVAILABLE: UnavailableObservations = UnavailableObservations {
    usage: (),
    sessions: (),
    models: (),
    cost: (),
    quota: (),
};

#[tauri::command]
pub(crate) fn abf_get_synthetic_snapshot() -> SyntheticSnapshot {
    SyntheticSnapshot {
        schema_version: 1,
        provenance: "fixed-synthetic",
        sources: [
            SyntheticSource {
                id: SourceId::Windows,
                observations: UNAVAILABLE,
            },
            SyntheticSource {
                id: SourceId::SelectedWsl,
                observations: UNAVAILABLE,
            },
        ],
    }
}
