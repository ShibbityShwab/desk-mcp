# Policy Engine

desk-mcp includes a declarative, YAML-based security policy engine that
evaluates every tool call **before** it is dispatched. You can use it to:

- **Allow** or **deny** specific tools outright.
- **Require confirmation** before potentially destructive actions.
- **Conditionally block** dangerous commands, restricted parameter values, or
  untrusted browser domains.
- **Cap** domains, filesystem paths, or timeouts per tool.
- **Limit** the overall session (max actions, max duration, idle timeout).

The policy is defined in a single YAML file:

```
~/.config/desk-mcp/policy.yaml
```

If no file exists, desk-mcp falls back to a **built-in default** that is
reasonably permissive: reads are allowed unconditionally, writes require
manual confirmation, and obviously dangerous shell patterns are blocked.

> **Reloading**: The policy file is read once at startup and cached for the
> lifetime of the process. To apply changes you must **restart desk-mcp**.

---

## Policy structure

```yaml
version: "1.0"
default: allow               # or "deny"
rules:                        # evaluated in order (first-match-wins semantics)
  - allow: [...]              # explicit allow list
    deny: [...]               # explicit deny list
    require_confirmation: [...]   # tools that need user approval
    auto_approve_after: 5         # auto-approve after N manual approvals
    deny_unless: [...]            # conditional deny rules
    cap: [...]                    # capability caps (domains, paths, timeout)
session:
  max_actions: 500                # tool calls per session
  max_duration_minutes: 30        # session lifetime
  require_reauth_after_idle_minutes: 15
```

### `version`

Currently `"1.0"`. Reserved for future schema evolution.

### `default`

Either `allow` or `deny`. Controls what happens when no rule matches a tool
call.

- **`allow`** — permissive: unknown tools are allowed.
- **`deny`** — restrictive: unknown tools are blocked. You must explicitly
  `allow` every tool you want to use.

### `rules`

An ordered list of rule blocks. Each rule is evaluated in sequence. The
evaluation order within a single rule is:

1. **Explicit deny** — immediate deny (always wins across rules too).
2. **`deny_unless` conditions** — immediate deny if a condition fails.
3. **Capability caps** — immediate deny if a cap is violated.
4. **Explicit allow** — recorded, but a later rule's deny overrides it.
5. **`require_confirmation`** — recorded if no allow was found yet.

### `session`

Global session-limit settings (see [Session limits](#session-limits) below).

---

## Default (built-in) configuration

When no `~/.config/desk-mcp/policy.yaml` exists, desk-mcp uses this:

```yaml
version: "1.0"
default: allow
rules:
  - require_confirmation:
      - shell_run
      - file_write
      - file_edit
      - code_run
      - browser_download
    auto_approve_after: 5
  - deny_unless:
      - tool: shell_run
        condition:
          command_not_contains:
            - "rm -rf"
            - "sudo"
            - "mkfs"
            - "dd if="
            - "> /dev/"
            - "chmod 777"
            - ":(){ :|:& };:"
        reason: "Dangerous shell command blocked by policy"
session:
  max_actions: 500
  max_duration_minutes: 30
  require_reauth_after_idle_minutes: 15
```

What this means in practice:

| Tool | Behaviour |
|---|---|
| `screenshot`, `read_file`, `list_dir`, `grep_files`, `web_search` | Allowed silently |
| `shell_run`, `file_write`, `file_edit`, `code_run`, `browser_download` | Requires confirmation (auto-approved after 5th use) |
| `shell_run` containing `sudo`, `rm -rf`, `mkfs`, etc. | **Denied** unconditionally |

---

## Current permissive config

The file found at `~/.config/desk-mcp/policy.yaml` (if you have one) may look
like this — effectively disabling all restrictions:

```yaml
# desk-mcp permissive policy — all guard rails disabled
version: "1.0"
default: allow
rules: []
session:
  max_actions: 1000000
  max_duration_minutes: 1000000
  require_reauth_after_idle_minutes: 1000000
```

Rules are empty, so every tool is allowed with no confirmation. Session limits
are set to effectively-infinite values.

---

## How to restrict

### 1. Deny specific tools

Add a `deny` list to a rule:

```yaml
rules:
  - deny:
      - shell_run
      - code_run
```

Now `shell_run` and `code_run` are always blocked — no override possible.

### 2. Require confirmation for certain tools

```yaml
rules:
  - require_confirmation:
      - file_write
      - file_edit
      - browser_download
```

desk-mcp will prompt the user before executing these tools.

### 3. Auto-approve after N confirmations

When you need confirmation for a tool but don't want to click every time:

```yaml
rules:
  - require_confirmation:
      - shell_run
      - file_write
    auto_approve_after: 3
```

After the user has manually confirmed 3 invocations of `shell_run` (within the
same session), subsequent calls are automatically approved. A different tool
(`file_write`) tracks its own counter.

### 4. Conditional deny (`deny_unless`)

`deny_unless` lets you block a tool unless certain conditions are met.

#### a. Block dangerous shell commands (`command_not_contains`)

```yaml
rules:
  - deny_unless:
      - tool: shell_run
        condition:
          command_not_contains:
            - "rm -rf"
            - "sudo"
            - "mkfs"
            - "dd if="
            - "> /dev/"
            - "chmod 777"
        reason: "Dangerous shell command blocked by policy"
```

The `command_not_contains` condition **denies** the call if the `command`
parameter contains any of the listed substrings. This is the built-in
default — it blocks `sudo rm -rf /` while allowing `cargo build`.

#### b. Block parameter values (`params_contains`)

Block specific parameter values for any tool:

```yaml
rules:
  - deny_unless:
      - tool: browser_launch
        condition:
          params_contains:
            field: mode
            values:
              - headless
              - stealth
        reason: "Headless/stealth browser mode not allowed"
```

The `ParamsContains` condition **denies** the call if the named parameter
(`mode`) matches any of the listed values.

#### c. Domain allowlist (`domain_not_in`)

Restrict browser navigation to trusted domains only:

```yaml
rules:
  - deny_unless:
      - tool: browser_navigate
        condition:
          domain_not_in:
            - example.com
            - docs.python.org
            - github.com
        reason: "Domain not in browser allowlist"
```

The `DomainNotIn` condition **denies** navigation if the extracted domain is
not in the allowlist. Subdomains of an allowed domain are also permitted
(e.g. `sub.example.com` is allowed when `example.com` is in the list).

### 5. Capability caps (`cap`)

Caps place upper bounds on what a tool can do. Unlike `deny_unless` (which
matches on specific parameter values), caps enforce **limits** on domains,
file paths, and timeouts.

#### Domain cap

```yaml
rules:
  - cap:
      - tool: web_fetch
        domains:
          - api.example.com
          - docs.rs
```

This limits `web_fetch` to those domains only. Any URL outside the list is
denied.

#### Filesystem path cap

```yaml
rules:
  - cap:
      - tool: file_write
        paths:
          - /tmp/desk-mcp/
          - /home/user/sandbox/
```

Allows `file_write` only when the path argument starts with one of the listed
prefixes.

#### Timeout cap

```yaml
rules:
  - cap:
      - tool: shell_run
        max_duration_secs: 60
```

Any `shell_run` with a `timeout` parameter exceeding 60 seconds is denied.

---

## Session limits

Three global knobs control how long a desk-mcp session can live:

| Field | Default | Description |
|---|---|---|
| `max_actions` | `500` | Maximum tool calls per session. Resets when desk-mcp restarts. |
| `max_duration_minutes` | `30` | Session lifetime in minutes. Resets on restart. |
| `require_reauth_after_idle_minutes` | `15` | Minutes of inactivity before the user must re-authorise. |

Example — strict session limits:

```yaml
session:
  max_actions: 100
  max_duration_minutes: 60
  require_reauth_after_idle_minutes: 5
```

When a limit is reached, subsequent tool calls return a `Deny` response with a
descriptive message.

---

## Combining rules

Rules are evaluated in order. You can use multiple rule blocks for layered
security:

```yaml
version: "1.0"
default: deny
rules:
  # 1. Allow read-only tools unconditionally
  - allow:
      - read_file
      - list_dir
      - grep_files
      - web_search

  # 2. Block dangerous shell commands
  - deny_unless:
      - tool: shell_run
        condition:
          command_not_contains:
            - "sudo"
            - "rm -rf"
        reason: "Dangerous shell command blocked"

  # 3. Require confirmation for writes, but auto-approve after 3
  - require_confirmation:
      - file_write
      - file_edit
      - shell_run
    auto_approve_after: 3

  # 4. Restrict browser to trusted domains only
  - cap:
      - tool: browser_navigate
        domains:
          - github.com
          - docs.rs

session:
  max_actions: 200
  max_duration_minutes: 120
```

In this example:

- `default: deny` means any tool not explicitly allowed is blocked.
- Rule 1 allows reads without confirmation.
- Rule 2 conditionally blocks `sudo` and `rm -rf` in shell commands (this
  runs before rule 3, so dangerous commands are denied even if `shell_run` is
  otherwise allowed).
- Rule 3 requires confirmation for `file_write`, `file_edit`, and
  `shell_run`, auto-approving after 3 manual approvals per tool.
- Rule 4 caps browser navigation to trusted domains.
- Session expires after 200 actions or 120 minutes.

---

## Complete reference

### `PolicyConfig`

| Field | Type | Default | Description |
|---|---|---|---|
| `version` | string | `"1.0"` | Schema version |
| `default` | `allow` / `deny` | `allow` | Default action when no rule matches |
| `rules` | list of `PolicyRule` | `[]` | Ordered evaluation rules |
| `session` | `SessionPolicy` | — | Session limits |

### `PolicyRule`

| Field | Type | Default | Description |
|---|---|---|---|
| `allow` | list of strings | `[]` | Explicitly allow these tools |
| `deny` | list of strings | `[]` | Explicitly deny these tools |
| `require_confirmation` | list of strings | `[]` | Require user confirmation for these tools |
| `auto_approve_after` | integer or null | `null` | After N manual confirmations, auto-approve |
| `deny_unless` | list of `DenyCondition` | `[]` | Conditional deny rules |
| `cap` | list of `CapRule` | `[]` | Capability caps |

### `DenyCondition`

| Field | Type | Description |
|---|---|---|
| `tool` | string | Tool name to evaluate |
| `condition` | object | One of `command_not_contains`, `params_contains`, or `domain_not_in` |
| `reason` | string or null | Custom denial message |

### `DenyConditionType`

| Variant | Shape | Behaviour |
|---|---|---|
| `command_not_contains` | list of strings | Deny if the `command` param contains any listed substring |
| `params_contains` | `{ field, values }` | Deny if the named parameter equals any listed value |
| `domain_not_in` | list of strings | Deny if the URL domain is not in the allowlist |

### `CapRule`

| Field | Type | Description |
|---|---|---|
| `tool` | string | Tool name |
| `domains` | list of strings | Allowed domains (empty = no restriction) |
| `paths` | list of strings | Allowed filesystem path prefixes (empty = no restriction) |
| `max_duration_secs` | integer or null | Maximum timeout for the tool |

### `SessionPolicy`

| Field | Type | Default | Description |
|---|---|---|---|
| `max_actions` | integer | `500` | Maximum tool calls per session |
| `max_duration_minutes` | integer | `30` | Maximum session duration in minutes |
| `require_reauth_after_idle_minutes` | integer | `15` | Idle timeout before re-auth required |

---

## Evaluation order (detailed)

The engine processes rules in the order they appear in the `rules` list.
Within each rule, the check order is fixed:

1. **Explicit `deny`** — if the tool is in the `deny` list, return
   `Deny`. This is a hard block — it overrides everything, including
   explicit `allow` in other rules.
2. **`deny_unless` conditions** — for each condition, if the tool matches
   and the condition fails, return `Deny`.
3. **`cap` checks** — for each cap, if the tool matches and a cap is
   exceeded, return `Deny`.
4. **Explicit `allow`** — if the tool is in the `allow` list, record
   `Allow` but continue scanning (a later rule's deny overrides this).
5. **`require_confirmation`** — if the tool is in the list and no allow
   was recorded yet, record `RequireConfirmation`.
6. After all rules are processed, return the best recorded decision, or the
   `default` if nothing matched.
