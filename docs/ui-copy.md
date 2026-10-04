# How the window talks

**Plain, active and specific.** Every label, button, empty state and error in charter says what
is true, in words the reader already has, and says what to do next when there is something to
do. This page writes down how the window already talks, so new copy matches it. Every example
below is a string the app ships today.

`docs/design-system.md` covers how the window looks. ADR 0072 covers which nouns it may use. This
page covers how it puts them into sentences. ST9's definition of done asks that every feature's
empty state and error copy follow it.

## The voice

- **charter is lowercase**, at the start of a sentence too. It is the program's name
  (`productName`), and it does the acting: *charter could not read the alerts*, *charter checks
  on its own every few hours*, *This is charter {version}*. The one capitalised form is the
  About dialog's title, *About Charter*, because there it is a title. Whether to keep even that
  exception is still open (#1156).
- **The reader is "you".** *You have not opened a project yet.* *Nothing is written into your
  repo.*
- **Active and present.** Say who does what: *It checks again when you press Delete*, not
  *a check will be performed*.
- **Say what is true, then stop.** One claim per sentence. Don't use "simply", "just", "please"
  or "successfully". A save that worked says *Session saved*, not *Session saved successfully!*
- **No exclamation marks**, and no emoji.
- **Plain words for the result, not the mechanism.** *Its value is gone from the vault for good.*
  If the reader needs a mechanism to decide, name it in one clause: *It needs sudo, so charter
  types it in a shell tab and leaves running it to you.*

## Case

- **Sentence case everywhere:** buttons, menu items, headings, tab titles, dialog titles,
  placeholders and `aria-label`s. Write *Create workspace*, *Open project…* and *Recent
  projects*, never *Open Project…*.
- **Names keep their own capitals:** GitHub, GitLab, Claude Code, Codex, opencode, Keychain,
  Touch ID.
- **A control named inside a sentence is written as its label reads**, with no quotes: *start
  one and press Check again*, *Make one with the + above, or New vault… in the palette*. The
  capital tells the reader it is something to press.

## Buttons and menu items

- **A verb and what it acts on:** *Create branch*, *Delete vault*, *Restart now*, *Open in its
  pane*. A button that confirms names the act it confirms. It is never *OK* or *Yes*.
- **Cancel is the way back** and changes nothing. A second way out says how it differs: *Wait*
  beside *Restart now*, *Keep it off* beside *Turn the sandbox on*.
- **An ellipsis means "asks something first".** *New project…*, *Browse…*, *Add an extension…*
  open a dialog or a picker before anything happens. A button that acts at once has none.
  Always the one character `…`, never three dots.
- **The same act has the same words everywhere:** the palette row, the menu item, the button
  and the empty state's way out. The window's catalogue (`actions.ts`) is where each act's
  label is written once.

## Work in progress

- **A present participle and an ellipsis:** *Reading the workspace…*, *Asking git…*, *Copying
  your repo into its workspace…*. Say what is being read or asked, not just *Loading…*.
- Something that is not done yet is *Not read yet.* or *not written yet*, not blank.

## Empty states

`EmptyState.tsx` draws them, and its fields are the rule:

- **The headline is a claim about what is true, never an instruction.** *No workspaces yet*,
  *No chats in this workspace*, *Nothing needs you here.*, *Nothing remembered yet*. Use "yet"
  when the thing is expected to arrive.
- **The body is the way out, named the way the window names it:** *Make one with the + above,
  or New persona… in the palette.*, *Add one in the box above.*, *charter runs each chat in its own pane. Open
  the first one here.*
- **Never the storage underneath.** Leave out file layouts, store names and ADR numbers. *Todos
  are files in this workspace's store* told the reader nothing they could act on.
- **An empty state with no way out is fine.** When there is nothing the reader can do, say so
  and stop. Don't invent an action.
- **Something that has gone says it has gone:** *This memory is not here any more*, *Nothing
  is at {path} now.*

## Errors and refusals

- **Say what happened, from charter's side, then why:** *charter could not list the branches of
  {repo}: {reason}*. The reason after the colon is the underlying error, passed through as it
  was said.
- **Then what to do, when there is something to do:** *Could not clone {repos} into {workspace}
  — {reason} Retry from the workspace's settings.* A message with nothing to do ends after the
  reason.
- **Never a stock phrase.** *Something went wrong*, *An error occurred*, *Unknown error*,
  *Oops* and a leading *Error:* each say that nothing was found out. If charter really does not
  know, say what it was doing: *charter did not answer with a reading*.
- **A refusal says what charter will not do, and why:** *charter cannot send {key}.*, *charter
  will not read …*. If it does something else instead, say that too: *{name} did not start
  ({reason}). It is still recorded, and will be tried again at the next launch.*
  (`PlaneView.tsx`, the notice for a chat that did not reopen).
- **Don't blame the reader.** The subject is what failed, not what the reader did wrong.
- **Say the cost of an act that cannot be undone, before it happens:** *There is no undo.*,
  *Its 2 secrets are destroyed in your system keychain and cannot be recovered.*
- **Uncertainty is stated, not hidden:** *{name} reports no state, so charter cannot tell
  whether it is mid-turn.*

## Words

- **The nouns are ADR 0072's.** Five concepts (Project, Workspace, Chat, Persona, Memory), and
  on the first-hour surfaces only those plus *Save* and *branch*. `CONTEXT.md` defines each
  word and lists the ones to avoid. A code repository is always a **repo**, on every forge,
  never a project.
- **No jargon in the window:** no ADR or ticket numbers, no internal type, module or store
  names, no protocol names where the effect can be said instead.
- **A command is written in code font** (`<code>`) where the command line is the way to do
  something: *You can add it later with <code>charter workspace vision</code>.* Only name
  commands the `charter` binary has. A panel's text from Rust is plain text and cannot carry
  code font, so a command there reads as raw backticks (#1156).
- **Counts are digits, and plurals agree:** *1 secret*, *2 secrets* (`counted` in
  `Vaults.tsx`).
- **An `aria-label` says what the visible label says**, and an icon-only control's label is the
  verb it would have had as text: *Close find*, *Next match*.

## What the build checks

`app/src/copy.test.ts` reads every string in `app/src` (`uiStrings.ts` finds them with
TypeScript's parser) and fails on:

- **a stock phrase**, in any string: *something went wrong*, *an error occurred*, *unknown
  error*, *oops*, *please*, *successfully*, or a leading *error:*;
- **an exclamation mark** at the end of text the window shows;
- **title case** in text the window shows: two or more words of four letters or more, all
  capitalised. Names (`NAMES` in `copy.ts`) and key chords such as `Ctrl+Shift+F` are taken
  out first. A name the check does not know fails on its first label, and adding it to
  `NAMES` is the fix;
- **a capital "Charter"** in text the window shows, anywhere but the About dialog's title.

"Text the window shows" is:

- JSX text, and a JSX child in braces;
- the value of an attribute a person reads or hears: `aria-label`, `title`, `placeholder`,
  `alt`, `label`, and `EmptyState`'s `headline` and `body`;
- a `label:` property;
- the catalogue's titles and reasons: the second argument of `can` and `cannot` in
  `actions.ts`, and the third of `cannot`.

Through an expression, both branches of a conditional and the right-hand side of `&&`, `||` and
`??` count as shown; the condition does not. Copy written in Rust and sent to the window, such
as a panel's empty state, is outside the check. So is copy assembled from parts at run time. DS-8's audit of every surface reads what the check cannot,
and a review reads every new string against this page.
