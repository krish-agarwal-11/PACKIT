# Stage 3B change summary

Application production behavior and version are unchanged; only a cfg(test) audit probe was added to the native adapter.

- Updated README.md, THIRD_PARTY_NOTICES.md, package-lock.json (existing root version alignment only).
- Updated legal/RELEASE_REVIEW.md, SEVEN_ZIP_REVIEW.md, WINDOWS_NATIVE_REVIEW.md, SOURCE_DISTRIBUTION.md, TOOLCHAIN_REVIEW.md, DEVELOPMENT_ASSETS.md, COPYRIGHT.md, DEPENDENCY_LICENSES.md, inventory.json, evidence-manifest.json, NATIVE_COMPONENTS.json and first-party-spdx.json.
- Added legal/MPL_REVIEW.md, mpl-source-review.json, 7zip-release-provenance.json, frontend-source-provenance.json, npm-source-claims.json, distribution-exclusions.json, recovery-evidence.json, recovery-downloads.json, upstream/recovered/ exact texts and sources/{mpl,7zip}/ exact archives.
- Added scripts/recover_license_evidence.py, recover_exact_notices.py, apply_license_resolutions.py, prepare_frontend_sources.py, prepare_source_bundle.py, restore_test_fixtures.py and validate_compliance.py. Updated audit_licenses.py to recognize explicit removed-asset exclusions without regenerating the audit.
- Added src-tauri/src/archive/license_release_probe.rs and its cfg(test) module declaration in seven_zip.rs. The probe uses the existing contained decoder; no decoder, extractor, shell, UI or registry behavior was replaced.
- Removed five unused non-approved source assets and two stale generated dist copies, enumerated with hashes in distribution-exclusions.json. Approved branding is unchanged. Thirteen uncertain RAR fixtures are locally preserved but excluded from generated distribution sources.
- Internal downloaded evidence, source candidates, extracted official binary comparisons and quarantined files are under ignored experiments/license-audit/. No source service, installer, version promotion, binding written offer or publication was created.
