# Security Policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's "Report a
vulnerability" feature in the Security tab of this repository, instead of
opening a public issue.

I try to acknowledge reports within 72 hours and will keep you informed while
a fix is prepared.

## Scope

ulanzi-dial is a local desktop application:

- It makes no network connections and collects no telemetry.
- The sensitive surfaces are local: the evdev exclusive grab, the virtual
  uinput device, the session D-Bus name it listens to
  (`dev.ulanzi.Dial.Focus`) and the user-configured shell commands executed
  for custom actions.

## Supported versions

Only the latest `main` branch receives security fixes.
