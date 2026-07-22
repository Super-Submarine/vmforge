# UAT / AT step-ID reconciliation vs the frozen CLI (v1.0-beta)

**Inputs:** onboarding kit `019f8a61-054f` (AT-1…AT-5; coverage map F5→AT-1,
F1→AT-2, F2→AT-3, F3→AT-4, F4→AT-5), Beta Program Plan v1 `019f8a52-1122` §A
(UAT-1…UAT-6, whose step tables the kit's AT scripts mirror), and
`docs/cli-freeze-v1.0-beta.md`.

**Method note:** the kit's full-script attachment is access-restricted, so
step-level reconciliation is done against the beta-plan UAT tables the kit was
built from (per the kit's own coverage map and assumption A3 "provisional CLI
verbs — freeze step IDs with engineering before wave 1"). Any kit step that
deviates from the plan tables should be re-checked against §2 by the kit owner
(Lena) — the verb-level diffs below apply either way.

## 1. Verdict summary

| Script | Kit ID | Plan ID | Verdict |
|---|---|---|---|
| Install & first launch (F5) | AT-1 | UAT-1 | **KEEP** — no CLI dependency; blocked on packaging (GAP-3), not on the freeze |
| Create VM (F1) | AT-2 | UAT-2 | **KEEP** — GUI-driven; CLI not referenced |
| Boot & lifecycle (F2) | AT-3 | UAT-3 | **KEEP, 1 step flagged** — 3.5 pause/resume has no frozen CLI verb (GUI/engine only) |
| Snapshot & restore (F3) | AT-4 | UAT-4 | **KEEP** — GUI-driven; CLI snapshot equivalents are experimental |
| Headless CLI parity (F4) | AT-5 | UAT-5 | **AMEND** — 4 of 6 steps reference verbs/flags that are not in the frozen surface (diff list §2) |
| SSH port-forward | — (kit A7) | UAT-6 | **OUT of wave 1** — descoped by decision (§3); remove from the wave-1 golden-path list |

Step **IDs** (numbering) are frozen as-is for all kept scripts — issue
references like `UAT-2.4` stay valid. Only the step **contents** listed below
need amending.

## 2. Diff list (step contents vs frozen surface)

| Step ID | Script says | Frozen surface says | Required amendment |
|---|---|---|---|
| UAT-5.1 / AT-5.1 | `vmforge list --json` | `list` is experimental (PR #3); `--json` exists on **no** `vmforge` verb (contract §4 aspiration only) | Until PR #3 merges + `--json` lands: mark step BLOCKED-on-M1; interim machine-readable check is `vmforge-storage snapshot list <vm> <disk> --json` (stable) |
| UAT-5.2 / AT-5.2 | `vmforge create` a second VM from curated image, exit 0 | `create` is experimental; frozen shape (PR #3) takes `--disk PATH` (+optional `--disk-size`), has **no `--image`** curated-image flag (contract §4 delta) | Reword to `vmforge create <name> --disk <path> --disk-size 8G`; drop "curated image" from the CLI step; BLOCKED-on-M1 |
| UAT-5.3 / AT-5.3 | `vmforge start <vm> --headless` | No `--headless` flag exists on any branch; PR #3 `start` is headless by default (QMP+serial, no console window) | Delete the flag: `vmforge start <vm>` already satisfies the pass criterion |
| UAT-5.4 / AT-5.4 | `vmforge snapshot take/restore/list <vm>` | Frozen sub-verb set (PR #3, experimental) is `snapshot create|restore|delete|list`; **`take` does not exist**; `create/restore/delete` also require `<tag>` | Rename `take`→`create`; add `<tag>` argument; BLOCKED-on-M1 |
| UAT-5.5 / AT-5.5 | invalid command → "machine-parseable error, documented nonzero exit" | Frozen today: exit 2 + human text for unknown verbs (`main`); PR #3: exit 1 + `error: ...` text; JSON `{"error": ...}` on stderr is contract §4, **not implemented** | Relax pass criterion to "documented nonzero exit code" (2 today, 1 at M1); machine-parseable JSON error moves to post-freeze promotion |
| UAT-5.6 / AT-5.6 | scripted loop incl. `delete` | `vmforge delete <vm>` exists on **no** branch (contract §4 only) | Drop `delete` from the loop or substitute `vmforge-storage delete <vm> <disk> --force` (stable) for disk cleanup; BLOCKED-on-M1 for the rest |
| UAT-3.5 / AT-3.5 | Pause, then resume | No `pause`/`resume` CLI verb frozen or on PR #3 (contract §4 only); GUI/engine capability per HAL | Keep as a GUI step; annotate "no CLI equivalent in v1.0-beta" |
| UAT-6.1–6.5 | SSH port-forward via "UI toggle or `vmforge` flag" | No `--forward` flag on any `vmforge` verb; hostfwd exists only in experimental `vmforge-net` (PR #2, unmerged) | **Descoped from wave 1** — see §3 |

Interim golden path for testers stays `qa/smoke/smoke_test.sh` (stable, §3 of
the freeze doc), exactly as `docs/tester-guide/` (PR #11) documents.

## 3. UAT-6 scope decision

**Decision: UAT-6 (port-forwarded SSH) is OUT of wave 1.** Recorded on the
control plane via `POST /api/decisions` (decision ID posted to the vmforge
channel with this doc). Rationale: no frozen user-facing surface exists (no
`--forward` flag, `vmforge-net` unmerged/experimental), wave 1 is Linux-only
with the smoke-suite golden path, and beta-plan assumption A2 pre-authorized
exactly this amendment ("if engineering disagrees, UAT-6 moves to the first
post-MVP milestone and the golden-path list should be amended"). Wave-1
release gate is therefore UAT-1→5; UAT-6 re-enters at wave 2 once PR #2 lands
behind a frozen `vmforge` flag.
