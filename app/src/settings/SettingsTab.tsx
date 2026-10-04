import { useMemo, useState } from "react";
import { atCreation } from "../windowprefs";
import { SettingGroup, SettingRow, SettingsLayout } from "./components";
import { LEVELS, type Level, type Setting, type SettingsGroup } from "./groups";
import { youGroups } from "./you";

/**
 * **Settings** (SE-16, #1166; the spec on #558, rulings V89a–i): the one tab where a setting is
 * read and changed, one level at a time (`CONTEXT.md`, **Settings** and **Level**). A view tab
 * (`tabs.settingsView`), opened from the app menu's Settings…, `⌘,` and the palette.
 *
 * **Two columns.** The level switcher and what that level is sit at the top; the left nav lists
 * the level's groups; the right column shows the chosen group and nothing else. Only **You** is
 * offered yet: Project (SE-17), Workspace (SE-20) and Persona join the switcher as their groups
 * land, each declared as data the same way (`groups.ts`).
 *
 * "Preferences" was this tab's You level before it had a name (charter-app#283), and is retired.
 */
export function SettingsTab() {
  const level: Level = "you";
  const groups = useMemo(() => youGroups(), []);
  const [chosen, choose] = useState(groups[0].id);
  const group = groups.find((one) => one.id === chosen) ?? groups[0];
  const where = atCreation().layout.path || "the layout file";
  return (
    <SettingsLayout
      levels={LEVELS.filter((one) => one.id === level)}
      level={level}
      onLevelChange={() => undefined}
      about={`This machine only, in every project. Kept in ${where}.`}
      groups={groups}
      group={group.id}
      onGroupChange={choose}
    >
      <ShownGroup key={group.id} group={group} />
    </SettingsLayout>
  );
}

/** The chosen group, drawn from its data. Keyed by the group, so each setting's hook is always
 *  the same one in a given row. */
function ShownGroup({ group }: { group: SettingsGroup }) {
  return (
    <SettingGroup label={group.label} help={group.help}>
      {group.settings.map((setting) => (
        <ShownSetting key={setting.id} setting={setting} />
      ))}
    </SettingGroup>
  );
}

function ShownSetting({ setting }: { setting: Setting }) {
  const { control, reset } = setting.useControl();
  return <SettingRow label={setting.label} help={setting.help} reset={reset} control={control} />;
}
