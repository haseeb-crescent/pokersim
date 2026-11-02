// The ten Learn concepts, in order. Modules are mapped to slugs in LearnPage.

export interface Concept {
  slug: string
  title: string
  blurb: string
  /** needs the local server for part of the page */
  server?: boolean
}

export const CONCEPTS: Concept[] = [
  { slug: 'pot-odds', title: 'Pot odds', blurb: 'What price a call is getting, and the equity it needs.' },
  { slug: 'equity', title: 'Equity', blurb: 'Your share of the pot: outs, hand vs hand and hand vs range.', server: true },
  { slug: 'ev', title: 'Expected value', blurb: 'Averaging outcomes to compare fold, call, bet and raise.' },
  { slug: 'ranges-position', title: 'Ranges & position', blurb: 'Thinking in combos, and why acting last is worth so much.' },
  { slug: 'preflop-charts', title: 'Preflop charts', blurb: 'Reading the solver charts: opens, defends and mixed cells.', server: true },
  { slug: 'range-nut-advantage', title: 'Range & nut advantage', blurb: 'Whose range a board favours, and why that sets the strategy.', server: true },
  { slug: 'mdf-bluff-ratios', title: 'MDF & bluff ratios', blurb: 'How often to defend, and how many bluffs a bet can carry.' },
  { slug: 'bet-sizing-polarization', title: 'Bet sizing & polarization', blurb: 'Small vs big bets, range shapes and geometric sizing.' },
  { slug: 'blockers', title: 'Blockers', blurb: 'Card removal: counting what villain can still hold.' },
  { slug: 'exploitation', title: 'Exploitation', blurb: 'From equilibrium to best response against real opponents.' },
]

export const CONCEPT_BY_SLUG: Record<string, Concept> = Object.fromEntries(CONCEPTS.map((c) => [c.slug, c]))
