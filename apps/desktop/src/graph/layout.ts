// Force layout (d3-force): links pull a child toward its folder, nodes push each other apart and
// never overlap. It keeps velocities between updates, so new nodes settle in without the whole
// picture jumping. Positions are written back to the graph on every tick.

import {
  forceCollide, forceLink, forceManyBody, forceSimulation, forceX, forceY,
  type Simulation, type SimulationLinkDatum, type SimulationNodeDatum,
} from "d3-force";
import type Graph from "graphology";

interface LNode extends SimulationNodeDatum {
  id: string;
  r: number;
  /** Held in place for a while so new nodes settle around a still picture. */
  soft?: boolean;
}
interface LLink extends SimulationLinkDatum<LNode> {
  gap: number;
}

type G = Graph<Record<string, unknown>, Record<string, unknown>>;

export class ForceLayout {
  private sim: Simulation<LNode, LLink>;
  private byId = new Map<string, LNode>();
  private active = false;
  private softTimer = 0;

  constructor(private readonly graph: G) {
    this.sim = forceSimulation<LNode, LLink>([])
      .velocityDecay(0.42)
      .alphaDecay(0.028)
      .alphaMin(0.004)
      .force("link", forceLink<LNode, LLink>([]).id((d) => d.id).distance((l) => {
        const s = l.source as LNode;
        const t = l.target as LNode;
        return s.r + t.r + l.gap;
      }).strength(0.85))
      .force("charge", forceManyBody<LNode>().strength((d) => -(14 + 5 * d.r)).distanceMax(900).theta(0.9))
      .force("collide", forceCollide<LNode>((d) => d.r + 1.5).strength(1).iterations(2))
      .force("x", forceX<LNode>(0).strength(0.012))
      .force("y", forceY<LNode>(0).strength(0.012))
      .on("tick", () => this.write())
      .on("end", () => (this.active = false))
      .stop();
  }

  /**
   * Read nodes and links from the graph again (after nodes were added or removed).
   * `local`: nodes that were already there hold still for a moment, so only the new ones move
   * (the picture you are looking at does not drift away when a folder opens).
   */
  rebuild(local = false): void {
    const g = this.graph;
    const next = new Map<string, LNode>();
    g.forEachNode((key, a) => {
      // Nodes on their way out (folding back into their folder) are animated, not laid out.
      if (a.leaving) return;
      const old = this.byId.get(key);
      const n: LNode = old ?? { id: key, r: 1, x: a.x as number, y: a.y as number };
      n.r = a.size as number;
      n.soft = false;
      if (a.fixed) {
        n.fx = a.x as number;
        n.fy = a.y as number;
      } else if (local && old) {
        n.fx = n.x;
        n.fy = n.y;
        n.soft = true;
      } else {
        n.fx = null;
        n.fy = null;
      }
      next.set(key, n);
    });
    clearTimeout(this.softTimer);
    if (local) this.softTimer = window.setTimeout(() => this.releaseSoft(), 1600);
    this.byId = next;
    const links: LLink[] = [];
    g.forEachEdge((_e, a, s, t) => {
      if (!next.has(s) || !next.has(t)) return;
      links.push({ source: s, target: t, gap: a.weight === 0.5 ? 4 : 7 });
    });
    this.sim.nodes([...next.values()]);
    (this.sim.force("link") as ReturnType<typeof forceLink<LNode, LLink>>).links(links);
  }

  /** Let held nodes move again, gently. */
  private releaseSoft(): void {
    for (const n of this.byId.values()) {
      if (n.soft) {
        n.fx = null;
        n.fy = null;
        n.soft = false;
      }
    }
    this.heat(0.04);
  }

  /** Sizes changed but not the structure. */
  resize(): void {
    this.graph.forEachNode((key, a) => {
      const n = this.byId.get(key);
      if (n) n.r = a.size as number;
    });
    (this.sim.force("collide") as ReturnType<typeof forceCollide<LNode>>).radius((d) => d.r + 1.5);
  }

  /** Start or warm up the simulation. */
  heat(alpha: number): void {
    this.sim.alpha(Math.max(this.sim.alpha(), alpha));
    if (!this.active) {
      this.active = true;
      this.sim.restart();
    }
  }

  running(): boolean {
    return this.active;
  }

  stop(): void {
    this.sim.stop();
    this.active = false;
  }

  /** Hold a node at a point (while dragging). */
  hold(key: string, x: number, y: number): void {
    const n = this.byId.get(key);
    if (!n) return;
    n.fx = x;
    n.fy = y;
    this.sim.alphaTarget(0.12);
    this.heat(0.12);
  }

  /** Stop dragging; `pin` keeps the node where it is. */
  release(key: string, pin: boolean): void {
    const n = this.byId.get(key);
    this.sim.alphaTarget(0);
    if (n && !pin) {
      n.fx = null;
      n.fy = null;
    }
  }

  unpin(key: string): void {
    const n = this.byId.get(key);
    if (n) {
      n.fx = null;
      n.fy = null;
      n.soft = false;
    }
    this.heat(0.3);
  }

  private write(): void {
    const g = this.graph;
    g.updateEachNodeAttributes((key, a) => {
      const n = this.byId.get(key);
      if (!n || n.x === undefined || n.y === undefined) return a;
      a.x = n.x;
      a.y = n.y;
      return a;
    }, { attributes: ["x", "y"] });
  }
}
