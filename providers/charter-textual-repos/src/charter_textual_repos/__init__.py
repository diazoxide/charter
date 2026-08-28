"""A charter frame component provider, built with Textual — the experiment.

charter's own ``pyproject.toml`` says ``dependencies = []`` and
`tests/test_packaging.py::test_runtime_has_zero_dependencies` asserts it. This
distribution declares ``textual`` and its ten transitive dependencies as **its own**, and
charter finds it through ``importlib.metadata.entry_points(group="charter.components")``,
which is stdlib. Nothing in charter's tree changes to make this work, which is the
property the experiment was run to test.

**Two components, because the experiment is a comparison.**

``textual.repos``
    The adapter. A Textual app on the headless driver, its composited screen copied out
    as plain lines once per repaint. Obeys `render(ctx) -> list[str]` exactly as written,
    and loses colour and every form of input on the way out — see :mod:`.adapter`.

``textual.live``
    The takeover. ``render`` starts a Textual app on the pane's own tty and never
    returns. Keys, mouse and scrolling are real; the snapshot is frozen at the moment the
    pane started, because a `ctx` is one repaint's snapshot and nothing calls `render`
    again — see :mod:`.live`.

**This module imports `charter.frame.component`, and that is worth saying out loud.** The
contract is a Python dataclass, checked with ``isinstance`` in
`frame/registry.py:Providers._one`, so a provider cannot express a component without
importing charter. It is not a cost — charter is already in the process that asks — but it
does mean ``charter-cp`` is a real install-time dependency of every provider, and the
entry-point group alone does not make the coupling loose.
"""

from __future__ import annotations

from charter.frame.component import Component, Fixed

#: The contract's version, read by `frame/registry.py:Providers._one` **before** the entry
#: point's own attribute is looked up. Charter refuses a provider declaring a different
#: integer rather than negotiating one (§4g), and the refusal names this distribution.
API_VERSION = 1

#: What both components ask charter for: the whole gather snapshot, which is what
#: charter's own `repos` component declares and for the reason `frame/builtins.py` gives —
#: the narrower ``repos`` slice flattens "nothing has scanned yet" and "this workspace has
#: no clones" to the same ``()``, and the table has to tell them apart.
NEEDS = ("gather",)

#: The rectangle either component takes when nothing says otherwise — a `Registry.place`
#: with no ``edge``/``size`` passed, which is the shape `frame/panel.py:run` calls it in.
#:
#: A committed ``[[frame.component]]`` table beats this and must carry both keys
#: (`instance.component_tables`), and when it does, the size it carries is coerced to
#: ``Fixed(n)``. So a provider's ``Content()`` or ``Fill()`` is reachable only on a frame
#: nobody has configured — which is the second place the contract chafed, and it is
#: recorded in the report rather than worked around here.
DEFAULT_EDGE = "bottom"
DEFAULT_SIZE = Fixed(12)


def adapter_component() -> Component:
    """``textual.repos`` — Textual rendered headless, returned as lines.

    ``events=()`` is the truthful declaration and not an omission. `EVENT_KINDS`' own
    docstring says a declaration is "what you HANDLE, never a promise that it FIRES";
    this component handles nothing, because a headless app has no terminal to read from
    and charter has no dispatcher to read one for it.
    """
    from . import adapter
    return Component(
        id="textual.repos",
        title="repos (textual, headless)",
        edge=DEFAULT_EDGE,
        size=DEFAULT_SIZE,
        needs=NEEDS,
        events=(),
        render=adapter.render,
    )


def live_component() -> Component:
    """``textual.live`` — Textual owning the pane, on its own loop.

    ``events`` declares the four kinds the app genuinely handles. **Charter reads this
    declaration and does nothing with it**: `component.names` validates it at
    construction and no other production line in charter references
    ``Component.events``. The app receives its keys and its mouse reports from the tty
    directly, having taken the pane; the declaration is documentation with no mechanism
    behind it, which is finding #1's sharpest edge.
    """
    from . import live
    return Component(
        id="textual.live",
        title="repos (textual, live)",
        edge=DEFAULT_EDGE,
        size=DEFAULT_SIZE,
        needs=NEEDS,
        events=("key", "click", "scroll", "resize"),
        render=live.render,
    )


__all__ = ["API_VERSION", "adapter_component", "live_component"]
