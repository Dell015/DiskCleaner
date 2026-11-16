// What kind of app each installed app is (games, launchers, developer tools, ...), for the
// Library view. Windows does not record this, so it comes from a list of well-known names and
// publishers plus a few clues the scan already has (a launcher installed it, it is a Windows
// part). An app nothing recognises is "other": never a guess. Display only: a kind never changes
// what is offered for removing.

import { isSystemPart } from "./apps";
import type { Tree } from "./tree";

export type AppKind = "game" | "launcher" | "dev" | "creative" | "browser" | "chat" | "office" | "ai" | "media" | "hardware" | "security" | "utility" | "system" | "other";

/** Every kind, in the order the Library shows them. */
export const APP_KINDS: AppKind[] = ["game", "launcher", "dev", "creative", "browser", "chat", "office", "ai", "media", "hardware", "security", "utility", "other", "system"];

export const KIND_TEXT: Record<AppKind, { title: string; one: string; about: string }> = {
  game: { title: "Games", one: "Game", about: "Games, wherever they are installed. A game a launcher installed says which one." },
  launcher: { title: "Launchers", one: "Launcher", about: "Stores that install and update games. Each one lists the games it installed." },
  dev: { title: "Developer tools", one: "Developer tool", about: "Code editors, languages, databases, containers and other programming tools." },
  creative: { title: "Creative", one: "Creative", about: "Video, photo, design, music and streaming apps." },
  browser: { title: "Browsers", one: "Browser", about: "Web browsers." },
  chat: { title: "Chat & meetings", one: "Chat & meetings", about: "Messaging, voice and video call apps." },
  office: { title: "Office & cloud", one: "Office & cloud", about: "Documents, notes, PDFs and cloud storage." },
  ai: { title: "AI assistants", one: "AI assistant", about: "Chat assistants and other AI apps." },
  media: { title: "Music & video players", one: "Player", about: "Apps that play music, videos and photos." },
  hardware: { title: "Hardware & drivers apps", one: "Hardware app", about: "Apps from the makers of your graphics card, motherboard, mouse, keyboard and laptop." },
  security: { title: "Security & VPN", one: "Security", about: "Antivirus and VPN apps." },
  utility: { title: "Utilities", one: "Utility", about: "Tools for files, archives, downloads, screenshots, remote help and tuning." },
  other: { title: "Other apps", one: "Other", about: "Apps Rhizome does not recognise. Right-click a card to put it where it belongs." },
  system: { title: "System parts", one: "System part", about: "Drivers, runtimes and Windows components other apps need. Leave them be." },
};

/** Launchers, by app name. Checked before games: "Steam" is a launcher, its games are games. */
const LAUNCHER = /\b(steam|epic games launcher|riot client|hoyoplay|battle\.net|ea app|origin|ubisoft connect|uplay|gog galaxy|xbox app|rockstar games launcher|amazon games|itch|tlauncher|minecraft launcher|curseforge|modrinth|prism launcher|playnite|wutheringwaves launcher|garena|bluestacks|ldplayer|noxplayer|memu)\b/i;

/** Named apps, by kind (checked in this order, before publishers). */
const NAMED: [AppKind, RegExp][] = [
  ["ai", /\b(copilot|chatgpt|claude|amd chat|gemini|perplexity|ollama|lm studio|jan|gpt4all)\b/i],
  ["dev", /\b(visual studio|vs ?code|vscodium|cursor|windsurf|kiro|zed|jetbrains|intellij|pycharm|webstorm|rider|clion|goland|android studio|docker|podman|wsl|git|github desktop|gitkraken|sourcetree|python|anaconda|miniconda|node\.?js|nvm|rust|rustup|go programming|golang|java\b|jdk|jre|temurin|openjdk|eclipse|netbeans|mysql|mariadb|postgres(ql)?|sql server|mongodb|redis|dbeaver|heidisql|xampp|wamp|laragon|postman|insomnia|wireshark|putty|winscp|filezilla|notepad\+\+|sublime text|cmake|msys2|mingw|cygwin|arduino|unity hub|unity \d|unreal engine|godot|roblox studio|scratch|processing|matlab|r for windows|rstudio|flutter|dart|composer|php|ruby|perl|strawberry|terminal|powershell \d|windows terminal|vagrant|virtualbox|vmware|hyper-v|ngrok|tortoisegit|tortoisesvn|beyond compare|winmerge|hex editor|ida|ghidra|x64dbg)\b/i],
  ["creative", /\b(capcut|figma|obs studio|obs|streamlabs|adobe (photoshop|premiere|after effects|illustrator|lightroom|audition|animate|indesign|express|creative cloud|media encoder)|photoshop|premiere|after effects|lightroom|blender|davinci resolve|gimp|krita|paint\.net|inkscape|affinity|canva|clipchamp|filmora|vegas|shotcut|kdenlive|openshot|audacity|fl studio|ableton|reaper|bandlab|musescore|muse hub|cubase|studio one|logic|clip studio|medibang|aseprite|substance|cinema 4d|maya|3ds max|zbrush|sketchup|fusion 360|autocad|nvidia broadcast|voicemod|elgato|camtasia|bandicam|medal)\b/i],
  ["browser", /\b(chrome|chromium|brave|firefox|opera|edge|vivaldi|tor browser|arc|zen browser|waterfox|librewolf|yandex|maxthon|thorium)\b/i],
  ["chat", /\b(discord|zoom|teams|slack|telegram|whatsapp|messenger|skype|viber|signal|webex|line|wechat|kakaotalk|element|guilded|teamspeak|mumble)\b/i],
  ["office", /\b(microsoft (office|365)|office 16|office home|onenote|word|excel|powerpoint|outlook|libreoffice|openoffice|wps office|onedrive|dropbox|google drive|icloud|mega(sync)?|box drive|notion|obsidian|evernote|acrobat|adobe reader|foxit|sumatra|pdf|todoist|trello|anki|zotero|mendeley|grammarly|thunderbird)\b/i],
  ["media", /\b(spotify|vlc|itunes|apple music|apple tv|netflix|mpc-hc|mpc-be|potplayer|foobar|winamp|aimp|musicbee|media player|k-lite|plex|jellyfin|kodi|stremio|photos|deezer|tidal|youtube music|amazon music|audible|kindle|calibre)\b/i],
  ["security", /\b(mcafee|norton|avast|avg|kaspersky|bitdefender|malwarebytes|eset|sophos|trend micro|panda|webroot|windows defender|nordvpn|protonvpn|proton vpn|expressvpn|surfshark|cloudflare warp|1\.1\.1\.1|mullvad|windscribe|tunnelbear|cyberghost|private internet access|hotspot shield|bitwarden|1password|keepass|lastpass|dashlane|glasswire|simplewall)\b/i],
  ["hardware", /\b(g hub|lghub|logitech|armoury crate|ai suite|asus|razer|synapse|icue|corsair|steelseries|hyperx|ngenuity|geforce|nvidia|radeon|adrenalin|amd|intel|realtek|msi center|dragon center|afterburner|gigabyte|aorus|rgb fusion|signalrgb|openrgb|samsung magician|crucial|western digital|wd dashboard|seagate|lenovo vantage|legion|dell|alienware|hp (support|smart)|omen|acer|predator|nitrosense|creative|sound blaster|focusrite|elgato|stream deck|wacom|huion|xp-pen|tobii|8bitdo|ds4windows|ryzen master)\b/i],
  ["utility", /\b(wallpaper engine|7-zip|winrar|peazip|bandizip|everything|powertoys|ccleaner|revo|iobit|driver booster|wise|glary|tenorshare|imazing|3utools|anydesk|teamviewer|rustdesk|parsec|rufus|ventoy|balena|cpu-z|gpu-z|hwinfo|hwmonitor|crystaldisk|speccy|qbittorrent|utorrent|bittorrent|deluge|internet download manager|idm|free download manager|jdownloader|sharex|lightshot|greenshot|snagit|windirstat|wiztree|treesize|spacesniffer|rainmeter|f\.lux|flux|autohotkey|keepass|translucenttb|startallback|open-shell|start11|fences|ditto|quicklook|files app|directory opus|total commander|easeus|aomei|macrium|acronis|minitool|recuva|handbrake|ffmpeg|yt-dlp|4k (video )?downloader|nero|imgburn|poweriso|daemon tools|winzip|unlocker|process (explorer|lasso)|sysinternals|autoruns|wireguard|openvpn|hamachi|radmin|zerotier|tailscale|localsend|nearby share|phone link|your phone|samsung (flow|smart switch)|smart switch|airdroid|scrcpy|droidcam|iriun|ldplayer)\b/i],
];

/** Game makers and stores (publisher), for games no launcher record names. */
const GAME_PUBLISHER = /\b(riot games|roblox|mihoyo|cognosphere|hoyoverse|mojang|electronic arts|ubisoft|blizzard|activision|rockstar|bandai namco|cd projekt|capcom|sega|square enix|bethesda|valve|kuro games|nexon|garena|tencent games|netease|krafton|bungie|epic games|devolver|paradox|focus entertainment|team cherry|re-logic|innersloth|landfall|hypixel|smilegate|pearl abyss|wargaming|gaijin|digital extremes|grinding gear|playstation|xbox game studios|nintendo|level-5|koei tecmo|konami|505 games|thq|deep silver|annapurna|humble|amazon games)\b/i;

/** Hardware makers (publisher). */
const HARDWARE_PUBLISHER = /\b(advanced micro devices|amd|nvidia|intel|realtek|asustek|asus|logitech|razer|corsair|steelseries|hyperx|micro-star|msi|gigabyte|lenovo|dell|hp inc|hewlett|acer|samsung electronics|western digital|seagate|kingston|crucial|creative technology|elgato|wacom|tobii|synaptics|elan|qualcomm|mediatek|broadcom|dolby|dts|waves audio|nahimic|a-volute|conexant|ite tech)\b/i;

/** Developer tool makers (publisher). */
const DEV_PUBLISHER = /\b(jetbrains|docker|oracle|python software foundation|node\.?js|the git development community|github|gitlab|postman|mongodb|mariadb|postgresql|eclipse|adoptium|azul|amazon corretto|unity technologies|epic games, inc\.?\s*unreal|rust|anaconda|wireshark)\b/i;

/** Creative tool makers (publisher). */
const CREATIVE_PUBLISHER = /\b(adobe|blackmagic|image-line|ableton|corel|figma|canva|bytedance|capcut|autodesk|maxon|foundry|serif|techsmith|magix|wondershare|muse group|obs project|blender foundation)\b/i;

/** Where games live (an app whose install folder is here is a game). */
const GAME_FOLDER = /\\(steamapps\\common|epic games|riot games|games|xboxgames|gog games|ea games|ubisoft game launcher\\games|hoyoplay\\games)\\/i;

/** The kind of an installed app (an owner of kind "app"). `overrides` maps app names to kinds the person chose. */
export function appKind(tree: Tree, owner: number, overrides?: Record<string, AppKind>): AppKind {
  const o = tree.owners[owner];
  if (!o) return "other";
  const chosen = overrides?.[o.name];
  if (chosen && APP_KINDS.includes(chosen)) return chosen;
  return guessKind(o.name, o.publisher, o.folder, o.launcher);
}

/** The kind from what Windows knows (no person's choice). Exported for tests. */
export function guessKind(name: string, publisher?: string, folder?: string, launcher?: string): AppKind {
  const pub = publisher ?? "";
  if (isSystemPart(name, publisher)) return "system";
  if (LAUNCHER.test(name)) return "launcher";
  // Something a launcher installed is a game (Steam also installs tools, but rarely big ones).
  if (launcher) return "game";
  for (const [kind, re] of NAMED) if (re.test(name)) return kind;
  if (GAME_PUBLISHER.test(pub)) return "game";
  if (DEV_PUBLISHER.test(pub)) return "dev";
  if (CREATIVE_PUBLISHER.test(pub)) return "creative";
  if (HARDWARE_PUBLISHER.test(pub)) return "hardware";
  if (folder && GAME_FOLDER.test(`${folder.replace(/\//g, "\\")}\\`)) return "game";
  return "other";
}
