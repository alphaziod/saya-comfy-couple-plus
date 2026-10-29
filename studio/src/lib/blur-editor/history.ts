// Bounded undo/redo history: pure logic, no DOM, tested in Node.
// States are (small) stroke documents; rendering replays the strokes.

export interface HistoryState<T> {
  readonly current: T;
  readonly past: readonly T[];
  readonly future: readonly T[];
}

export const HISTORY_LIMIT = 50;

export function initialHistory<T>(current: T, limit = HISTORY_LIMIT): HistoryState<T> {
  if (!Number.isInteger(limit) || limit < 1) throw new Error("Limite d'historique invalide.");
  return { current, past: [], future: [] };
}

export function pushHistory<T>(state: HistoryState<T>, next: T, limit = HISTORY_LIMIT): HistoryState<T> {
  const past = state.past.length >= limit ? state.past.slice(1) : state.past;
  return { current: next, past: [...past, state.current], future: [] };
}

export function undoHistory<T>(state: HistoryState<T>): HistoryState<T> {
  if (!state.past.length) return state;
  return { current: state.past[state.past.length - 1], past: state.past.slice(0, -1), future: [state.current, ...state.future] };
}

export function redoHistory<T>(state: HistoryState<T>): HistoryState<T> {
  if (!state.future.length) return state;
  return { current: state.future[0], past: [...state.past, state.current], future: state.future.slice(1) };
}

export function canUndo<T>(state: HistoryState<T>): boolean {
  return state.past.length > 0;
}
export function canRedo<T>(state: HistoryState<T>): boolean {
  return state.future.length > 0;
}
