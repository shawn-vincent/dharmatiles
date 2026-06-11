# Build Provenance Capture Specification

## Purpose

Applications should be able to identify precisely which source revision produced a running artifact, when that artifact was built, and whether it originated from a clean or modified source tree.

This information should be available at runtime without requiring access to source control systems, build servers, shell commands, local filesystems, or deployment infrastructure.

The goal is to capture provenance once during the build process and embed it permanently into the produced artifact.

---

# Goals

The system must allow any running artifact to answer:

* What version am I?
* Which source revision produced me?
* How far am I from the most recent release?
* Was the source tree modified when I was built?
* When was I built?
* What changes are included in this build?

The system should work consistently across:

* Web applications
* Backend services
* Serverless functions
* Mobile applications
* Desktop applications
* Command-line tools

---

# Non-Goals

The system is not intended to:

* Query source control systems at runtime
* Determine deployment timestamps
* Replace release management processes
* Depend on access to Git, Mercurial, SVN, or any specific source control tool after build completion

---

# Architecture

## Build-Time Capture

A build step executes before compilation.

The build step gathers metadata from:

* Source control
* Build environment
* Release tagging system

The metadata is written into a generated source file.

Example flow:

```text
Source Control
       ↓
Metadata Generator
       ↓
Generated Source File
       ↓
Compiler / Bundler
       ↓
Application Artifact
```

The generated file is considered a build artifact and should not be committed to source control.

A fresh copy is produced for every build.

---

## Runtime Consumption

Application code accesses build metadata through a stable interface.

Consumers should not depend on:

* Generated file locations
* Source control commands
* Build tooling implementation details

All runtime access occurs through a single exported object or service.

---

# Metadata Model

## BuildInfo

```ts
interface BuildInfo {
  version: string;
  commit: string;
  commitShort: string;

  branch: string;

  tag: string;
  commitsSinceTag: number;

  buildTimestamp: string;

  isDirty: boolean;

  commits: CommitInfo[];
}
```

---

## Field Definitions

### version

Human-readable build identifier.

Derived from source-control metadata.

Examples:

```text
v1.0.0
v1.0.0-12-gabc1234
v1.0.0-12-gabc1234-dirty
gabc1234
```

This field is the primary identifier displayed to users and support staff.

---

### commit

Full source revision identifier.

Example:

```text
74a4597b31d8e53d03c7f1e8b8d65d77d15f53e
```

---

### commitShort

Abbreviated commit identifier.

Example:

```text
74a4597
```

---

### branch

Source-control branch used during the build.

Examples:

```text
main
develop
feature/new-search
```

May be unavailable in detached-head environments.

---

### tag

Most recent reachable release tag.

Examples:

```text
v1.0.0
v2.4.3
```

May be empty or unknown if no tag is reachable.

---

### commitsSinceTag

Number of commits beyond the most recent reachable tag.

Examples:

```text
0
12
147
```

---

### buildTimestamp

UTC timestamp indicating when the artifact was produced.

Format:

```text
2026-06-11T09:34:12Z
```

This timestamp represents build time, not commit time.

This field is mandatory.

It is frequently one of the most valuable diagnostic fields because it allows operators to determine:

* Whether a deployment is stale
* Whether two systems are running the same build
* When a specific artifact was created

---

### isDirty

Indicates whether uncommitted changes were present during the build.

Values:

```text
true
false
```

A dirty build may not be reproducible from source control alone.

---

### commits

Optional collection of commit metadata since the previous release.

Can be used for:

* Release notes
* "What's New" screens
* Support diagnostics

---

# Version String Format

The canonical runtime display format shall be:

```text
Ver: <version> · <buildTimestamp>
```

Examples:

## Tagged Release

```text
Ver: v1.0.0 · 2026-06-11T09:34:12Z
```

## Tagged Release With Local Modifications

```text
Ver: v1.0.0-dirty · 2026-06-11T09:34:12Z
```

## Commits Beyond Release

```text
Ver: v1.0.0-12-gabc1234 · 2026-06-11T09:34:12Z
```

## Commits Beyond Release With Local Modifications

```text
Ver: v1.0.0-12-gabc1234-dirty · 2026-06-11T09:34:12Z
```

## Untagged Commit

```text
Ver: gabc1234 · 2026-06-11T09:34:12Z
```

## Untagged Commit With Local Modifications

```text
Ver: gabc1234-dirty · 2026-06-11T09:34:12Z
```

## Metadata Unavailable

```text
Ver: unknown · 2026-06-11T09:34:12Z
```

---

# Deployment Surface Integration

The same metadata model should be available to all deployment targets.

Implementation details may vary.

Examples include:

* Bundled source constants
* Compile-time environment variable injection
* Native application metadata generation
* Resource files
* Embedded manifests

All deployment targets should expose the same logical BuildInfo structure.

---

# Recommended Uses

## About Screens

```text
Ver: v1.0.0-12-gabc1234 · 2026-06-11T09:34:12Z
```

---

## Support Diagnostics

Include the complete BuildInfo payload.

---

## Error Tracking

Attach BuildInfo to telemetry events.

Examples:

* Sentry
* Application Insights
* Datadog
* Honeycomb

---

## Health Endpoints

Expose BuildInfo through administrative APIs.

Examples:

```text
GET /version
GET /health
```

---

## Release Notes

Generate release summaries from captured commit history.

---

## Deployment Verification

Validate that an environment is running the intended artifact.

---

# Failure Handling

Failure to retrieve source-control metadata must not fail the build.

Unknown values should be populated when metadata cannot be determined.

Example:

```json
{
  "version": "unknown",
  "commit": "unknown",
  "tag": "unknown"
}
```

Producing a runnable artifact is preferred over aborting the build.

---

# Design Principles

* Build-time generation, runtime consumption
* Immutable provenance after build completion
* Zero runtime source-control dependency
* Consistent across deployment targets
* Reproducible and deterministic
* Graceful degradation when metadata is unavailable
* Single source of truth for build identity

The system exists to ensure that every artifact can always identify exactly what it is and where it came from.
