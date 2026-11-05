# Windows native components — Stage 3B

## RESOLVED: loader identity and applicable SDK notice

The exact official NuGet package is Microsoft.Web.WebView2 **1.0.3800.47**:
https://api.nuget.org/v3-flatcontainer/microsoft.web.webview2/1.0.3800.47/microsoft.web.webview2.1.0.3800.47.nupkg

Package SHA-256: `56c9f26bdd07916a2d1949fb58a5c7e434dfa1173577dca879206050c4e718db`.
Its nuspec expressly identifies LICENSE.txt. The full SDK LICENSE.txt is Microsoft BSD-3-Clause; NOTICE.txt additionally preserves ANTLR 3 / StringTemplate 4 and Microsoft notices. All three original files are retained unchanged in upstream/recovered/microsoft-webview2-1.0.3800.47/. Preserve the entire NOTICE, copyright, conditions and disclaimers in a binary distribution. No endorsement is implied. This finding is specific to this exact SDK; it is not a blanket license for Microsoft products.

Nine cached loader files (DLL, import library, static library across x64/x86/arm64) match the official package byte-for-byte; NATIVE_COMPONENTS.json lists hashes. webview2-com-sys 0.39.1/src/lib.rs selects WebView2LoaderStatic under target_env=msvc. Thus the x64 loader code is statically linked into PACKIT; the cached DLL/import library are not separately configured runtime resources. Existing PE import evidence has no WebView2Loader.dll import. The wrapper MIT license is separate from the verified native BSD terms.

The installed Evergreen WebView2 Runtime and Windows OS components are external prerequisites, not bundled by this stage. Microsoft's [distribution guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution) distinguishes runtime deployment from the loader. If a future installer adds a runtime/bootstrapper or VC redistributable, audit its exact terms/payload separately. The current shell DLL imports VCRUNTIME140.dll; no VC redist is being shipped here.

## UNRESOLVED: narrow GPL source / System Library question

The downloaded SDK has no native loader implementation source. BSD-3-Clause permits redistribution under its conditions and is not itself the old assumed proprietary EULA obstacle. However, copying a native static library into a GPL executable raises a separate Corresponding Source question: is this loader a GPL section 1 System Library (including the Major Component/Standard Interface criteria), or must implementation source be supplied? Availability on Windows, Microsoft authorship, dynamic access to an external runtime and a permissive binary license do not by themselves prove the exemption. Preserve a qualified legal/Microsoft review before release; no special GPL exception is added and no unavailable source is promised.

A source package currently contains the original sys crate/native libraries plus exact SDK notices. That is evidence and build material, **not a claim that object libraries are preferred implementation source**. Do not remove WebView2 or change architectures speculatively.
