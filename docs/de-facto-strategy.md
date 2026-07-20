# TerminalBench De-facto Standard Strategy

As of: 2026-07-16
Owner: TerminalBench maintainers
Review trigger: quarterly; immediately after a wire/profile semantic change, a credible reproducibility failure, or the first independent producer adoption

## Decision and positioning

The first audience is terminal-emulator maintainers deciding whether and how to adopt TerminalBench report conformance in release and performance-regression workflows. Position TerminalBench as a vendor-neutral performance-reporting authority: a shared protocol for bounded claims, reproducible context, and pre-comparison eligibility. Do not position it as a fastest-terminal leaderboard.

Adopt the workflow incrementally: `run → validate → eligibility gate → compare`. Existing `tbench compare` remains useful for presentation and regression analysis, but a release claim is v1-safe only after both reports validate and the pair is `COMPARABLE`.

## Outcomes and delivery sequence

### 0–30 days: trustworthy local artifact

- Ship Reporting Specification v1, canonical JSON Schema, stdlib-only validator, fixtures, and generated-report tests together.
- Require every bundled metric to declare its exact method, observation boundary, limitations, and excluded claims.
- Publish a short maintainer recipe for local validation and retain reports as release artifacts.
- Baseline fixture pass rate and time-to-diagnose using stable rule IDs and JSON Pointers.

### 31–90 days: workflow adoption

- Add a copy-paste CI example that validates artifacts and runs eligibility before existing comparison.
- Recruit two terminal projects for private fixture feedback without offering ranking or endorsement.
- Define a change-control process: immutable published profiles, reviewed new profiles, versioned conformance corpus, and public rationale for semantic changes.
- Produce one cross-machine reproducibility study and label unresolved variance rather than normalizing it away.

### 12 months: category reference

- Target three independent terminal projects retaining conformant reports in release/regression workflows.
- Target one independent validator or producer implementation passing the corpus.
- Publish reproducibility envelopes for every profile and a transparent incompatibility/unknown-rate dashboard.
- Earn primary-source citations in release notes, performance investigations, or terminal documentation without operating a leaderboard.

### 36 months: durable neutral protocol

- Establish multi-vendor profile governance with documented recusals and compatibility policy.
- Maintain at least two independent implementations and a shared conformance suite.
- Make versioned performance reports a routine artifact of terminal releases and regression reports.
- Consider a neutral home or naming change only after governance and adoption evidence justify migration cost.

## KPIs

| Dimension | 90-day threshold | Accountable owner | Evidence source | 12-month target |
|---|---|---|---|---|
| Adoption | 2 independent terminal projects complete a documented validation/eligibility trial | Adoption owner | public issue/PR, or maintainer-approved trial record with validator output | 3 independent terminal projects retain conformant artifacts |
| Independent implementation | 1 non-TerminalBench producer or validator passes 100% of the published corpus | Conformance owner | implementation URL plus archived corpus output and version | 1 independently maintained producer/validator remains corpus-green |
| Reproducibility | 1 protocol run on at least 3 machine/terminal combinations with mismatch and unknown rates reported | Measurement owner | versioned raw reports, eligibility output, and study summary | profile-specific repeatability envelopes; unknown/mismatch rates tracked |
| Citation | 2 primary-source maintainer references to the specification or conformant reports | Adoption owner | release note, project documentation, issue, or PR URL | 5 primary-source release, issue, or documentation citations |

Counts are evidence, not self-awarded status. Actual external adoption and third-party neutrality recognition are future outcomes and currently **UNVERIFIED**.

## Governance

- The normative specification outranks schema and reference-validator implementations.
- Wire versions, profile versions, workloads, fixtures, and rule IDs are reviewed together.
- Published semantics are immutable; semantic changes require a new version or ID.
- Profile proposals must identify beneficiaries, method boundary, limitations, excluded claims, reproducibility evidence, and vendor-bias risks.
- Unknown extensions cannot change core validity or comparability. No project receives private exceptions.
- Maintainers with a direct vendor interest disclose it and recuse from disputed profile semantics.

### Minimum profile decision procedure

1. The conformance owner records a versioned proposal, corpus delta, evidence, known objections, and affected vendors. Normal review remains open for at least 14 days.
2. Every substantive objection receives a written disposition: accept, amend, defer with a dated evidence request, or reject with rationale. Unresolved trust or vendor-bias objections block approval.
3. Approval requires two non-recused maintainers, including the measurement owner or a named delegate. A maintainer employed by, paid by, or directly representing an affected terminal vendor discloses that interest and cannot supply either approval.
4. Emergency invalidation is allowed when a published profile permits overclaim, false comparability, or reproducible corruption. Two non-recused maintainers may immediately mark the profile invalid, publish scope/evidence and a rollback notice, and open the normal replacement review within 7 days. Existing semantics are never silently rewritten.

## Risks, mitigations, and kill criteria

| Risk | Mitigation | Kill / pause criterion |
|---|---|---|
| Proxy overclaim destroys trust | machine-enforced profile boundary and exact exclusions | any conformant path can call PTY completion on-screen rendering or PTY echo keypress-to-photon |
| Vendor-specific advantage | workload transparency, independent corpus, recusal | unresolved evidence that one implementation is structurally favored |
| Technically correct but unused | local→CI artifact→release gate adoption ladder | no maintainer completes a trial after two 90-day outreach/revision cycles |
| False comparability | explicit three-state gate; unknown never wildcard | gate returns comparable with a required unknown or known mismatch |
| Governance burden exceeds value | immutable small registry; measured demand before expansion | profile growth outpaces independent use or reproducibility evidence |
| Naming collision with the AI-agent `Terminal-Bench` project | park rename; avoid premature public brand expansion | revisit before broad publication, package registry investment, or neutral-foundation transfer |

If a trust kill criterion fires, stop adoption work, publish the affected scope, invalidate the relevant profile/version, and issue a corrected version only with a regression fixture. Do not quietly reinterpret published data.

## What this tranche does not prove

The v1 artifacts prove structural and semantic conformance of tested reports. They do not prove absolute measurement accuracy, all-terminal or all-OS conformance, independent neutrality, vendor adoption, or market leadership. Those remain explicitly **UNVERIFIED** until external evidence exists.
