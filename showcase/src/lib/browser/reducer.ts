import type { ActivityRecord, Node, TreeEntry } from "../shared/contracts";
import type { VirtualPath } from "../shared/path";
import type { BrowserStatus, ShowcaseError } from "./api";

export interface EditorState {
  path: VirtualPath | undefined;
  text: string;
  original: string;
  revision: number | undefined;
  dirty: boolean;
}

export interface RevisionConflict {
  path: VirtualPath;
  message: string;
}

export interface ShowcaseState {
  status: BrowserStatus | undefined;
  tree: readonly TreeEntry[];
  selectedPath: VirtualPath | undefined;
  selectedNode: Node | undefined;
  editor: EditorState;
  busyAction: string | undefined;
  dialogs: Readonly<Record<string, boolean>>;
  searchResults: unknown;
  trashResults: unknown;
  changesResults: unknown;
  activities: readonly ActivityRecord[];
  error: ShowcaseError | Error | undefined;
  revisionConflict: RevisionConflict | undefined;
}

export type ShowcaseAction =
  | { type: "status_loaded"; status: BrowserStatus }
  | { type: "tree_loaded"; entries: readonly TreeEntry[]; background: boolean }
  | { type: "selected"; entry: TreeEntry }
  | { type: "editor_loaded"; path: VirtualPath; text: string; revision: number }
  | { type: "editor_changed"; text: string }
  | { type: "busy_changed"; busyAction: string | undefined }
  | { type: "dialog_changed"; name: string; open: boolean }
  | { type: "search_loaded"; results: unknown }
  | { type: "trash_loaded"; results: unknown }
  | { type: "changes_loaded"; results: unknown }
  | { type: "activity_appended"; activity: ActivityRecord }
  | { type: "activities_cleared" }
  | { type: "error_set"; error: ShowcaseError | Error | undefined }
  | { type: "revision_conflict"; path: VirtualPath; message: string }
  | { type: "revision_conflict_cleared" };

export const initialShowcaseState: ShowcaseState = {
  status: undefined,
  tree: [],
  selectedPath: undefined,
  selectedNode: undefined,
  editor: {
    path: undefined,
    text: "",
    original: "",
    revision: undefined,
    dirty: false,
  },
  busyAction: undefined,
  dialogs: {},
  searchResults: undefined,
  trashResults: undefined,
  changesResults: undefined,
  activities: [],
  error: undefined,
  revisionConflict: undefined,
};

function nextEditorText(editor: EditorState, text: string): EditorState {
  return { ...editor, text, dirty: text !== editor.original };
}

/** Pure state transitions keep refreshes from clobbering a visitor's unsaved edit. */
export function showcaseReducer(
  state: ShowcaseState,
  action: ShowcaseAction,
): ShowcaseState {
  switch (action.type) {
    case "status_loaded":
      return { ...state, status: action.status };
    case "tree_loaded":
      return { ...state, tree: action.entries };
    case "selected":
      return {
        ...state,
        selectedPath: action.entry.path,
        selectedNode: action.entry.node,
        revisionConflict: undefined,
      };
    case "editor_loaded":
      if (state.editor.dirty && state.editor.path !== action.path) {
        return state;
      }
      return {
        ...state,
        editor: {
          path: action.path,
          text: action.text,
          original: action.text,
          revision: action.revision,
          dirty: false,
        },
        revisionConflict: undefined,
      };
    case "editor_changed":
      return { ...state, editor: nextEditorText(state.editor, action.text) };
    case "busy_changed":
      return { ...state, busyAction: action.busyAction };
    case "dialog_changed":
      return {
        ...state,
        dialogs: { ...state.dialogs, [action.name]: action.open },
      };
    case "search_loaded":
      return { ...state, searchResults: action.results };
    case "trash_loaded":
      return { ...state, trashResults: action.results };
    case "changes_loaded":
      return { ...state, changesResults: action.results };
    case "activity_appended":
      return { ...state, activities: [...state.activities, action.activity] };
    case "activities_cleared":
      return { ...state, activities: [] };
    case "error_set":
      return { ...state, error: action.error };
    case "revision_conflict":
      return {
        ...state,
        revisionConflict: { path: action.path, message: action.message },
      };
    case "revision_conflict_cleared":
      return { ...state, revisionConflict: undefined };
    default: {
      const exhaustive: never = action;
      return exhaustive;
    }
  }
}
