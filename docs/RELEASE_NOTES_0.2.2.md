# Kairo v0.2.2

- Clean ANSI management pages with persistent submenus, breadcrumbs and local error handling.
- Separate service, OAuth discovery, binding, deployment approval and observation status.
- Readable quota summaries, bounded journal pages, topic-based help and latest stable checks.
- Two-stage uninstall confirmation; data and existing scheduler safety gates remain protected.
- Bilingual PTY acceptance for navigation, repeated operations, EOF, SIGINT and NO_COLOR.

No scheduler algorithm, quota endpoint, model request or safety-gate changes. Existing CLI JSON and exit status behavior remain intact. Existing installations upgrade with `sudo kairo update online`; this release does not activate scheduling. Review missing deployment/history state independently before any activation.
