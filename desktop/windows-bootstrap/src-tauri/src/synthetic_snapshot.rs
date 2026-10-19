// Gate M6E DEMO TRANSPORT FIXTURE ONLY, not a production event/database contract.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyntheticSnapshot {
    schema_version: u8,
    provenance: &'static str,
    sources: [SyntheticSource; 2],
    records: [SyntheticRecord; 12],
}

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum SourceId {
    Windows,
    SelectedWsl,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum HarnessId {
    ClaudeCode,
    Codex,
    #[serde(rename = "opencode")]
    OpenCode,
}

#[derive(serde::Serialize)]
struct SyntheticSource {
    id: SourceId,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SyntheticRecord {
    id: &'static str,
    source: SourceId,
    harness: HarnessId,
    session_id: &'static str,
    equivalence_id: Option<&'static str>,
    model: &'static str,
    date: &'static str,
    input_tokens: u32,
    output_tokens: u32,
}

// Explicit equivalence is a fixture-only assertion, never inferred from values.
// Dates are fixed UTC day buckets. Tokens total input + output; no billing data.
#[allow(clippy::too_many_arguments)]
const fn record(
    id: &'static str,
    source: SourceId,
    harness: HarnessId,
    session_id: &'static str,
    equivalence_id: Option<&'static str>,
    model: &'static str,
    date: &'static str,
    input_tokens: u32,
    output_tokens: u32,
) -> SyntheticRecord {
    SyntheticRecord {
        id,
        source,
        harness,
        session_id,
        equivalence_id,
        model,
        date,
        input_tokens,
        output_tokens,
    }
}

#[tauri::command]
pub(crate) fn abf_get_synthetic_snapshot() -> SyntheticSnapshot {
    SyntheticSnapshot {
        schema_version: 2,
        provenance: "fixed-synthetic",
        sources: [
            SyntheticSource {
                id: SourceId::Windows,
            },
            SyntheticSource {
                id: SourceId::SelectedWsl,
            },
        ],
        records: [
            record(
                "demo-w1",
                SourceId::Windows,
                HarnessId::ClaudeCode,
                "demo-ws1",
                Some("demo-copy"),
                "demo-amber",
                "2026-01-01",
                10,
                5,
            ),
            record(
                "demo-w2",
                SourceId::Windows,
                HarnessId::ClaudeCode,
                "demo-ws2",
                None,
                "demo-amber",
                "2026-01-01",
                20,
                10,
            ),
            record(
                "demo-w3",
                SourceId::Windows,
                HarnessId::Codex,
                "demo-ws3",
                None,
                "demo-birch",
                "2026-01-02",
                30,
                10,
            ),
            record(
                "demo-w4",
                SourceId::Windows,
                HarnessId::Codex,
                "demo-ws4",
                None,
                "demo-birch",
                "2026-01-02",
                30,
                10,
            ),
            record(
                "demo-w5",
                SourceId::Windows,
                HarnessId::OpenCode,
                "demo-ws5",
                None,
                "demo-coral",
                "2026-01-03",
                40,
                10,
            ),
            record(
                "demo-w6",
                SourceId::Windows,
                HarnessId::OpenCode,
                "demo-ws5",
                None,
                "demo-coral",
                "2026-01-03",
                50,
                10,
            ),
            record(
                "demo-l1",
                SourceId::SelectedWsl,
                HarnessId::ClaudeCode,
                "demo-ls1",
                Some("demo-copy"),
                "demo-amber",
                "2026-01-01",
                10,
                5,
            ),
            record(
                "demo-l2",
                SourceId::SelectedWsl,
                HarnessId::ClaudeCode,
                "demo-ls2",
                None,
                "demo-amber",
                "2026-01-01",
                25,
                10,
            ),
            record(
                "demo-l3",
                SourceId::SelectedWsl,
                HarnessId::Codex,
                "demo-ls3",
                None,
                "demo-birch",
                "2026-01-02",
                45,
                10,
            ),
            record(
                "demo-l4",
                SourceId::SelectedWsl,
                HarnessId::Codex,
                "demo-ls3",
                None,
                "demo-birch",
                "2026-01-02",
                55,
                10,
            ),
            record(
                "demo-l5",
                SourceId::SelectedWsl,
                HarnessId::OpenCode,
                "demo-ls4",
                None,
                "demo-coral",
                "2026-01-03",
                65,
                10,
            ),
            record(
                "demo-l6",
                SourceId::SelectedWsl,
                HarnessId::OpenCode,
                "demo-ls4",
                None,
                "demo-coral",
                "2026-01-03",
                75,
                10,
            ),
        ],
    }
}
