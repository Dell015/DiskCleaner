// The order to uninstall things in (D133): a launcher's games and its other helpers (an
// anti-cheat driver, a service) first, the launcher itself last, so each one can still be removed
// by its own uninstaller. Pure: only reads the scan.

import { gamesOf } from "./apps";
import { guessKind, type AppKind } from "./kinds";
import type { Tree } from "./tree";

export interface Step {
  owner: number;
  /** What it is to the app being uninstalled ("" for the app itself). */
  role: string;
  /** A driver that runs from startup: restart after removing it. */
  restart: boolean;
}

/** Drivers and anti-cheat that load when Windows starts. */
const STARTUP_DRIVER = /\b(vanguard|anti-?cheat|easyanticheat|easy anti-cheat|battleye|faceit|vgk|ricochet|xigncode|nprotect|gameguard)\b/i;

/** "Riot Games, Inc." and "Riot Games Inc" are the same maker. */
function maker(p?: string): string {
  return (p ?? "").toLowerCase().replace(/[.,]/g, " ").replace(/\b(inc|ltd|llc|corp|corporation|co|gmbh|pte|limited|s a|sa)\b/g, "").replace(/\s+/g, " ").trim();
}

/** What to uninstall, in order, to remove this app properly. */
export function uninstallSteps(tree: Tree, owner: number, kind: AppKind): Step[] {
  const o = tree.owners[owner];
  const self: Step = { owner, role: "", restart: STARTUP_DRIVER.test(o.name) };
  if (kind !== "launcher") return [self];
  const games = gamesOf(tree, owner).filter((g) => tree.owners[g].kind === "app");
  const by = maker(o.publisher);
  const helpers: number[] = [];
  if (by) {
    tree.owners.forEach((x, i) => {
      if (i === owner || x.kind !== "app" || games.includes(i)) return;
      if (maker(x.publisher) !== by) return;
      if (guessKind(x.name, x.publisher) === "system") return;
      helpers.push(i);
    });
  }
  // Helpers that are games by name (another game from the same maker) go with the games.
  const asGames = helpers.filter((i) => !STARTUP_DRIVER.test(tree.owners[i].name));
  const drivers = helpers.filter((i) => STARTUP_DRIVER.test(tree.owners[i].name));
  return [
    ...[...games, ...asGames].map((g) => ({ owner: g, role: games.includes(g) ? "Game it installed" : `Also from ${o.publisher ?? o.name}`, restart: false })),
    ...drivers.map((d) => ({ owner: d, role: "Its anti-cheat or driver", restart: true })),
    { ...self, role: "Last" },
  ];
}

/** The scan entry at this path (case ignored), or -1. */
export function findPath(tree: Tree, path: string): number {
  const parts = path.replace(/\//g, "\\").split("\\").filter(Boolean);
  let at = 0;
  for (const part of parts) {
    const want = part.toLowerCase();
    let next = -1;
    for (let c = tree.firstChild[at]; c >= 0; c = tree.nextSibling[c]) {
      if (tree.names[c].replace(/[\\/]+$/, "").toLowerCase() === want) {
        next = c;
        break;
      }
    }
    if (next < 0) return -1;
    at = next;
  }
  return at === 0 ? -1 : at;
}
