/**
 * Subsequence fuzzy matching for the `Ctrl+P` jump. Pure and dependency-free
 * so it runs under node-environment vitest.
 *
 * The scoring is deliberately simple — enough to float the file you meant to
 * the top, not a general-purpose ranker. Preferences, in order: matches in
 * the file name over matches in the directory path, consecutive runs,
 * matches at a word boundary, and shorter paths.
 */

export interface FuzzyMatch {
  path: string;
  score: number;
  /** Indices in `path` that matched, for highlighting. */
  positions: number[];
}

// Consecutive outweighs word-boundary on purpose: typing "something" should
// find `something.ts` ahead of `s-o-m-e-t-h-i-n-g.ts`, where every character
// happens to sit on a boundary.
const BONUS_CONSECUTIVE = 12;
const BONUS_WORD_BOUNDARY = 10;
const BONUS_IN_FILENAME = 6;
const PENALTY_LEADING = 1;
const PENALTY_LENGTH = 0.05;

function isBoundary(previous: string | undefined): boolean {
  return previous === undefined || previous === "/" || previous === "_" || previous === "-";
}

/**
 * Score `path` against `query`, or `null` if the query isn't a subsequence of
 * it. An empty query matches everything with a neutral score.
 */
export function scorePath(path: string, query: string): FuzzyMatch | null {
  if (query.length === 0) {
    return { path, score: -path.length * PENALTY_LENGTH, positions: [] };
  }

  const haystack = path.toLowerCase();
  const needle = query.toLowerCase();
  const filenameStart = path.lastIndexOf("/") + 1;

  const positions: number[] = [];
  let score = 0;
  let cursor = 0;
  let previous = -1;

  for (const char of needle) {
    const found = haystack.indexOf(char, cursor);
    if (found === -1) return null;

    if (found === previous + 1 && previous !== -1) {
      score += BONUS_CONSECUTIVE;
    }
    if (isBoundary(path[found - 1])) {
      score += BONUS_WORD_BOUNDARY;
    }
    if (found >= filenameStart) {
      score += BONUS_IN_FILENAME;
    }
    positions.push(found);
    previous = found;
    cursor = found + 1;
  }

  score -= (positions[0] ?? 0) * PENALTY_LEADING;
  score -= path.length * PENALTY_LENGTH;
  return { path, score, positions };
}

/**
 * Best `limit` matches for `query`, highest score first. Ties break on path
 * so the ordering is stable between keystrokes.
 */
export function fuzzyFilter(paths: string[], query: string, limit: number): FuzzyMatch[] {
  const matches: FuzzyMatch[] = [];
  for (const path of paths) {
    const match = scorePath(path, query);
    if (match) matches.push(match);
  }
  matches.sort((a, b) => b.score - a.score || a.path.localeCompare(b.path));
  return matches.slice(0, limit);
}
