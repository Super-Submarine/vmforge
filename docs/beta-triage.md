# Beta issue triage & labels spec

Source of truth for how beta issues are labeled and triaged during the private beta.
Severity ladder and SLAs come from the week-1 operations runbook (readiness pack
`019f8a70-47bb`, §2) and the beta program plan (`019f8a52-1122` §D1).

## Severity ladder

| Label | Definition | SLA |
|---|---|---|
| `P1` | Data loss, snapshot-restore failure, or crash of a running VM. **Every restore failure is automatically P1** (M4 is a gate metric). | Ack < 24h; fix or workaround < 1 week. Same-day escalation to an engineering fix contract; wave-2 invites freeze until closed. |
| `P2` | A golden-path step (any UAT/AT step) fails but is recoverable. | Triage < 3 days. |
| `P3` | Paper cuts, UX polish, docs gaps. | Batched weekly in the Day-5 synthesis ritual. |

Severity is assigned at the **daily 15-minute triage sweep** (product + on-call eng),
with a continuous P1 watch. Chat is not the system of record: anything actionable is
promoted to a GitHub issue within 24 hours.

## Labels

### Severity (exactly one, applied at triage)

| Label | Color | Description |
|---|---|---|
| `P1` | `#b60205` | Data loss / restore failure / running-VM crash. Ack <24h. |
| `P2` | `#d93f0b` | Golden-path step fails, recoverable. Triage <3 days. |
| `P3` | `#fbca04` | Paper cut / UX / docs. Weekly batch. |

### Type (set by the template)

| Label | Color | Description |
|---|---|---|
| `bug` | `#d73a4a` | Defect report. |
| `feedback` | `#0e8a16` | Friction, feature request, UX suggestion. |
| `beta` | `#5319e7` | Filed by a beta tester during the beta program. |
| `needs-triage` | `#ededed` | Awaiting the daily triage sweep; removed once severity + feature labels are set. |

### Feature area (PRD F1–F5, applied at triage for weekly clustering)

| Label | Color | Description |
|---|---|---|
| `F1-create-boot` | `#1d76db` | VM create / first boot. |
| `F2-instant-resume` | `#1d76db` | Pause / instant resume. |
| `F3-snapshots` | `#1d76db` | Snapshot tree, branching, restore. |
| `F4-cli` | `#1d76db` | CLI verbs, flags, exit codes. |
| `F5-packaging` | `#1d76db` | Install, packaging, cross-host parity. |

### Workflow

| Label | Color | Description |
|---|---|---|
| `uat-fail` | `#c2e0c6` | Filed from a UAT/AT script FAIL; title/body must carry the step ID. |
| `wave-1` | `#bfdadc` | Reported by a wave-1 tester (week-1 metrics scope). |

## Triage flow

1. Issue filed via template (pre-fills host/version/step ID) → lands with `needs-triage`.
2. Daily sweep assigns exactly one severity label (`P1`/`P2`/`P3`) + one `F1`–`F5` label,
   removes `needs-triage`.
3. `P1` or gate-metric breach → same day, open an engineering fix contract on the control
   plane referencing the issue URL + UAT step; announce in the vmforge channel.
4. Fix lands → CI smoke + QA regression green → verified by the reporting tester →
   issue closed with the evidence link.
