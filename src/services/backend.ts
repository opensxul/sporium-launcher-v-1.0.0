import { invoke, isTauri } from '@tauri-apps/api/core';
import type {
  PackPreview,
  PackJob,
  PackExport,
  PackExportRequest,
  ExternalCandidate,
  Bootstrap,
  CommandError,
  ErrorCode,
  SaveSettingsRequest,
  SettingsSnapshot,
  CreateInstance,
  UpdateInstance,
  DuplicateInstance,
  DeleteInstance,
  SaveCollection,
  RecordRequest,
  InstanceLogo,
  InstanceSummary,
  OpenFolder,
  LibrarySnapshot,
  LibraryChange,
  SetFavorite,
  ShortcutPlan,
  VersionCatalog,
  JavaRuntime,
  GameRequest,
  GameState,
  EditProfile,
  ProfileSnapshot,
  SkinView,
  ConfigureLaunch,
  Loader,
  LoaderCatalog,
  CacheStats,
  CatalogQuery,
  CatalogPage,
  ContentTags,
  ContentDetails,
  ContentRequest,
  ContentCreateRequest,
  ContentPlan,
  ContentJob,
  InstalledContent,
  UntrackedContent,
  ContentChange,
  ContentHistory,
  LocalContentPlan,
  ContentAdoptionRequest,
  ContentAdoptionPlan,
  ModDiagnostics,
  ContentWorld,
  WorldArchiveRequest,
  WorldArchivePlan,
  WorldProjectRequest,
  LocalDependencyPlan,
  ContentSelection,
  ContentUpdate,
  ContentUpdateRequest,
  ContentUpdatePlan,
  ContentUpdatePolicy,
  ContentRestorePoint,
  AutomaticPolicy,
  AutomaticReport,
  ProjectView,
  ProjectFile,
  ProjectPlan,
  ProjectUpdate,
  StudioRequest,
} from '../bindings/core';

export type BackendErrorCode = ErrorCode | 'UNKNOWN' | 'DESKTOP_REQUIRED';

export class BackendError extends Error {
  constructor(
    public readonly code: BackendErrorCode,
    public readonly retryable: boolean,
  ) {
    super(code);
  }
}

const errorCodes: readonly ErrorCode[] = [
  'RATE_LIMITED',
  'CONTENT_INCOMPATIBLE',
  'CONTENT_UNSUPPORTED',
  'CONTENT_CONFLICT',
  'DEPENDENCY_CONFLICT',
  'STORAGE_UNAVAILABLE',
  'DATA_CORRUPT',
  'SCHEMA_TOO_NEW',
  'INVALID_SETTINGS',
  'SETTINGS_CONFLICT',
  'WORKER_UNAVAILABLE',
  'INVALID_INPUT',
  'NOT_FOUND',
  'RECORD_CONFLICT',
  'UNSAFE_PATH',
  'LIBRARY_BUSY',
  'SOURCE_CHANGED',
  'OPEN_FOLDER_FAILED',
  'NETWORK',
  'INTEGRITY',
  'UNSAFE_ARCHIVE',
  'CANCELLED',
  'UNSUPPORTED_VERSION',
  'JAVA_UNAVAILABLE',
  'INSTANCE_BUSY',
  'LAUNCH_FAILED',
];

export function normalizeError(error: unknown): BackendError {
  if (error instanceof BackendError) return error;
  if (
    typeof error === 'object' &&
    error !== null &&
    'code' in error &&
    typeof error.code === 'string' &&
    errorCodes.includes(error.code as ErrorCode)
  ) {
    const value = error as CommandError;
    return new BackendError(value.code, value.retryable === true);
  }
  // A raw IPC error can contain private local paths. Never display it verbatim.
  return new BackendError('UNKNOWN', true);
}

interface CommandMap {
  pack_preview: { args: { path: string }; result: PackPreview };
  provider_pack_preview: { args: { projectId: string; versionId: string }; result: PackPreview };
  pick_pack: { args: undefined; result: PackPreview | null };
  pack_import: { args: { token: string; name: string; optional: string[] }; result: PackJob };
  pack_dismiss: { args: { token: string }; result: void };
  pack_state: { args: undefined; result: PackJob | null };
  pack_cancel: { args: undefined; result: void };
  pack_opening: { args: undefined; result: string[] };
  external_scan: { args: { path: string }; result: ExternalCandidate[] };
  pick_external: { args: undefined; result: ExternalCandidate[] | null };
  external_preview: { args: { key: string }; result: PackPreview };
  pack_export: { args: { request: PackExportRequest; path: string }; result: PackExport };
  pick_pack_export: { args: { request: PackExportRequest }; result: PackExport | null };
  register_pack_formats: { args: undefined; result: void };
  content_worlds: { args: { id: string }; result: ContentWorld[] };
  world_archive_plan: { args: { request: WorldArchiveRequest }; result: WorldArchivePlan };
  pick_world_archive: { args: { request: WorldArchiveRequest }; result: WorldArchivePlan | null };
  finish_world_archive: {
    args: { token: string; acceptUnknown: boolean; cancel: boolean };
    result: void;
  };
  world_project_plan: { args: { request: WorldProjectRequest }; result: ContentPlan };
  local_content_dependencies: { args: { token: string }; result: LocalContentPlan };
  content_dependency_plan: {
    args: { id: string; files: ContentSelection[] };
    result: LocalDependencyPlan;
  };
  content_diagnostics: { args: { id: string }; result: ModDiagnostics };
  content_search: { args: { query: CatalogQuery }; result: CatalogPage };
  content_tags: { args: undefined; result: ContentTags };
  content_details: {
    args: { projectId: string; instanceId: string | null };
    result: ContentDetails;
  };
  content_plan: { args: { request: ContentRequest }; result: ContentPlan };
  content_icon: { args: { projectId: string }; result: string | null };
  content_create_plan: { args: { request: ContentCreateRequest }; result: ContentPlan };
  content_install: { args: { token: string }; result: ContentJob };
  content_state: { args: undefined; result: ContentJob | null };
  content_cancel: { args: undefined; result: null };
  installed_content: { args: { id: string }; result: InstalledContent[] };
  untracked_content: { args: { id: string }; result: UntrackedContent[] };
  change_content: { args: { request: ContentChange }; result: null };
  local_content_plan: { args: { id: string; paths: string[] }; result: LocalContentPlan };
  content_adoption_plan: { args: { request: ContentAdoptionRequest }; result: ContentAdoptionPlan };
  finish_content_adoption: {
    args: { token: string; acceptUnknown: boolean; cancel: boolean };
    result: null;
  };
  local_content_icon: {
    args: { id: string; directory: string; filename: string; sha512: string | null };
    result: string | null;
  };
  pick_local_content: { args: { id: string }; result: LocalContentPlan | null };
  finish_local_content: {
    args: { token: string; acceptUnknown: boolean; cancel: boolean };
    result: null;
  };
  content_history: { args: { id: string }; result: ContentHistory[] };
  content_updates: { args: { id: string }; result: ContentUpdate[] };
  content_restore_points: { args: { id: string }; result: ContentRestorePoint[] };
  restore_content: { args: { id: string; point: string; settings: boolean }; result: null };
  automatic_policy: { args: undefined; result: AutomaticPolicy };
  save_automatic_policy: { args: { value: AutomaticPolicy }; result: null };
  automatic_reports: { args: undefined; result: AutomaticReport[] };
  automatic_check: { args: { id: string }; result: AutomaticReport };
  project_view: { args: { id: string }; result: ProjectView };
  project_replan: { args: { token: string; groups: string[] }; result: ProjectPlan };
  studio_files: { args: { id: string }; result: ProjectFile[] };
  studio_plan: { args: { request: StudioRequest }; result: ProjectPlan };
  project_check: { args: { id: string }; result: ProjectUpdate };
  project_repair_plan: { args: { id: string }; result: ProjectPlan };
  project_pack_plan: { args: { id: string; versionId: string }; result: ProjectPlan };
  project_source_plan: { args: { id: string; groups: string[] }; result: ProjectPlan };
  project_pick_manifest: { args: { id: string; groups: string[] }; result: ProjectPlan | null };
  project_apply: { args: { token: string; acceptChanges: boolean }; result: string };
  project_dismiss: { args: { token: string }; result: null };
  project_cancel: { args: undefined; result: null };
  project_export: { args: { id: string }; result: boolean };
  content_update_plan: { args: { request: ContentUpdateRequest }; result: ContentUpdatePlan };
  content_update_policy: { args: { id: string; policy: ContentUpdatePolicy }; result: null };
  open_content_project: { args: { id: string }; result: null };
  pause_downloads: { args: { paused: boolean }; result: GameState };
  download_cache: { args: { cleanup: boolean }; result: CacheStats };
  edit_profile: { args: { request: EditProfile }; result: ProfileSnapshot };
  profile_skin: { args: { nickname: string; refresh: boolean }; result: SkinView };
  configure_instance_launch: { args: { request: ConfigureLaunch }; result: LibraryChange };
  loader_versions: { args: { loader: Loader; minecraft: string }; result: LoaderCatalog };
  game_catalog: { args: { refresh: boolean }; result: VersionCatalog };
  game_state: { args: undefined; result: GameState };
  start_game: { args: { request: GameRequest }; result: GameState };
  cancel_game_operation: { args: undefined; result: null };
  stop_game: { args: { id: string }; result: GameState };
  java_runtimes: { args: undefined; result: JavaRuntime[] };
  inspect_java: { args: { path: string }; result: JavaRuntime };
  bootstrap: { args: undefined; result: Bootstrap };
  save_settings: { args: { request: SaveSettingsRequest }; result: SettingsSnapshot };
  library_snapshot: { args: undefined; result: LibrarySnapshot };
  create_instance: { args: { request: CreateInstance }; result: LibraryChange };
  update_instance: { args: { request: UpdateInstance }; result: LibraryChange };
  duplicate_instance: { args: { request: DuplicateInstance }; result: LibraryChange };
  delete_instance: { args: { request: DeleteInstance }; result: LibraryChange };
  save_collection: { args: { request: SaveCollection }; result: LibraryChange };
  delete_collection: { args: { request: RecordRequest }; result: LibraryChange };
  open_library_folder: { args: { request: OpenFolder }; result: null };
  set_instance_favorite: { args: { request: SetFavorite }; result: LibraryChange };
  instance_shortcut_plan: { args: { request: RecordRequest }; result: ShortcutPlan };
  instance_icon: { args: { id: string }; result: string | null };
  instance_summary: { args: { id: string }; result: InstanceSummary };
  instance_icon_catalog: { args: undefined; result: InstanceLogo[] };
  set_instance_icon: { args: { request: RecordRequest; choice: string }; result: LibraryChange };
  pick_instance_icon: { args: { request: RecordRequest }; result: LibraryChange | null };
}

async function command<K extends keyof CommandMap>(
  name: K,
  args: CommandMap[K]['args'],
): Promise<CommandMap[K]['result']> {
  if (!isTauri()) throw new BackendError('DESKTOP_REQUIRED', false);
  try {
    return await invoke<CommandMap[K]['result']>(name, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

// Background receipt verification briefly holds a shared instance lease. Retry only
// lease contention; plans/tokens retain all compatibility and conflict checks.
async function contentOperation<T>(action: () => Promise<T>, attempts = 12): Promise<T> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await action();
    } catch (reason) {
      const error = normalizeError(reason);
      if (attempt >= attempts || !['INSTANCE_BUSY', 'LIBRARY_BUSY'].includes(error.code))
        throw reason;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }
}

// The library lock is acquired before mutations. Brief background reads may contend with it.
async function libraryOperation<T>(action: () => Promise<T>): Promise<T> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await action();
    } catch (reason) {
      if (attempt >= 12 || normalizeError(reason).code !== 'LIBRARY_BUSY') throw reason;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }
}

export const backend = {
  packPreview: (path: string) => command('pack_preview', { path }),
  providerPackPreview: (projectId: string, versionId: string) =>
    command('provider_pack_preview', { projectId, versionId }),
  pickPack: () => command('pick_pack', undefined),
  packImport: (token: string, name: string, optional: string[]) =>
    libraryOperation(() => command('pack_import', { token, name, optional })),
  packDismiss: (token: string) => command('pack_dismiss', { token }),
  packState: () => command('pack_state', undefined),
  packCancel: () => command('pack_cancel', undefined),
  packOpening: () => command('pack_opening', undefined),
  externalScan: (path: string) => command('external_scan', { path }),
  pickExternal: () => command('pick_external', undefined),
  externalPreview: (key: string) => command('external_preview', { key }),
  packExport: (request: PackExportRequest, path: string) =>
    contentOperation(() => command('pack_export', { request, path })),
  pickPackExport: (request: PackExportRequest) => command('pick_pack_export', { request }),
  registerPackFormats: () => command('register_pack_formats', undefined),
  contentDiagnostics: (id: string) =>
    contentOperation(() => command('content_diagnostics', { id })),
  contentWorlds: (id: string) => contentOperation(() => command('content_worlds', { id })),
  worldArchivePlan: (request: WorldArchiveRequest) =>
    contentOperation(() => command('world_archive_plan', { request })),
  pickWorldArchive: (request: WorldArchiveRequest) =>
    contentOperation(() => command('pick_world_archive', { request })),
  finishWorldArchive: (token: string, acceptUnknown: boolean, cancel = false) =>
    contentOperation(() => command('finish_world_archive', { token, acceptUnknown, cancel })),
  worldProjectPlan: (request: WorldProjectRequest) =>
    contentOperation(() => command('world_project_plan', { request })),
  localDependencies: (token: string) =>
    contentOperation(() => command('local_content_dependencies', { token })),
  contentDependencyPlan: (id: string, files: ContentSelection[]) =>
    contentOperation(() => command('content_dependency_plan', { id, files })),
  contentSearch: (query: CatalogQuery) =>
    libraryOperation(() => command('content_search', { query })),
  contentTags: () => command('content_tags', undefined),
  contentDetails: (projectId: string, instanceId: string | null = null) =>
    libraryOperation(() => command('content_details', { projectId, instanceId })),
  contentPlan: (request: ContentRequest) =>
    contentOperation(() => command('content_plan', { request })),
  contentIcon: (projectId: string) => command('content_icon', { projectId }),
  contentCreatePlan: (request: ContentCreateRequest) => command('content_create_plan', { request }),
  contentInstall: (token: string) => contentOperation(() => command('content_install', { token })),
  contentState: () => command('content_state', undefined),
  contentCancel: () => command('content_cancel', undefined),
  installedContent: (id: string) => contentOperation(() => command('installed_content', { id })),
  untrackedContent: (id: string) => contentOperation(() => command('untracked_content', { id })),
  changeContent: (request: ContentChange) =>
    contentOperation(() => command('change_content', { request })),
  contentHistory: (id: string) => contentOperation(() => command('content_history', { id })),
  contentUpdates: (id: string) => contentOperation(() => command('content_updates', { id })),
  automaticPolicy: () => command('automatic_policy', undefined),
  saveAutomaticPolicy: (value: AutomaticPolicy) => command('save_automatic_policy', { value }),
  automaticReports: () => command('automatic_reports', undefined),
  automaticCheck: (id: string) => contentOperation(() => command('automatic_check', { id })),
  projectView: (id: string) => contentOperation(() => command('project_view', { id })),
  projectReplan: (token: string, groups: string[]) =>
    contentOperation(() => command('project_replan', { token, groups })),
  studioFiles: (id: string) => contentOperation(() => command('studio_files', { id })),
  studioPlan: (request: StudioRequest) =>
    contentOperation(() => command('studio_plan', { request })),
  projectCheck: (id: string) => contentOperation(() => command('project_check', { id })),
  projectRepairPlan: (id: string) => contentOperation(() => command('project_repair_plan', { id })),
  projectPackPlan: (id: string, versionId: string) =>
    contentOperation(() => command('project_pack_plan', { id, versionId })),
  projectSourcePlan: (id: string, groups: string[]) =>
    contentOperation(() => command('project_source_plan', { id, groups })),
  projectPickManifest: (id: string, groups: string[]) =>
    command('project_pick_manifest', { id, groups }),
  projectApply: (token: string, acceptChanges: boolean) =>
    contentOperation(() => command('project_apply', { token, acceptChanges })),
  projectDismiss: (token: string) => command('project_dismiss', { token }),
  projectCancel: () => command('project_cancel', undefined),
  projectExport: (id: string) => command('project_export', { id }),
  contentRestorePoints: (id: string) =>
    contentOperation(() => command('content_restore_points', { id })),
  restoreContent: (id: string, point: string, settings = false) =>
    contentOperation(() => command('restore_content', { id, point, settings })),
  contentUpdatePlan: (request: ContentUpdateRequest) =>
    contentOperation(() => command('content_update_plan', { request })),
  contentUpdatePolicy: (id: string, policy: ContentUpdatePolicy) =>
    contentOperation(() => command('content_update_policy', { id, policy })),
  localContentPlan: (id: string, paths: string[]) =>
    contentOperation(() => command('local_content_plan', { id, paths })),
  pickLocalContent: (id: string) => command('pick_local_content', { id }),
  finishLocalContent: (token: string, acceptUnknown: boolean, cancel = false) =>
    contentOperation(() => command('finish_local_content', { token, acceptUnknown, cancel })),
  adoptionPlan: (request: ContentAdoptionRequest) =>
    contentOperation(() => command('content_adoption_plan', { request })),
  finishAdoption: (token: string, acceptUnknown: boolean, cancel = false) =>
    contentOperation(() => command('finish_content_adoption', { token, acceptUnknown, cancel })),
  localContentIcon: (id: string, directory: string, filename: string, sha512: string | null) =>
    contentOperation(() => command('local_content_icon', { id, directory, filename, sha512 })),
  openContentProject: (id: string) => command('open_content_project', { id }),
  pauseDownloads: (paused: boolean) => command('pause_downloads', { paused }),
  downloadCache: (cleanup = false) => command('download_cache', { cleanup }),
  editProfile: (request: EditProfile) => command('edit_profile', { request }),
  profileSkin: (nickname: string, refresh = false) =>
    command('profile_skin', { nickname, refresh }),
  configureLaunch: (request: ConfigureLaunch) =>
    libraryOperation(() => command('configure_instance_launch', { request })),
  loaderVersions: (loader: Loader, minecraft: string) =>
    command('loader_versions', { loader, minecraft }),
  gameCatalog: (refresh = false) => command('game_catalog', { refresh }),
  gameState: () => command('game_state', undefined),
  // A freshly opened large pack can still be verifying hundreds of files under a shared lease.
  // A busy rejection precedes every mutation; retry until the read releases its lease.
  startGame: (request: GameRequest) =>
    contentOperation(() => command('start_game', { request }), 240),
  cancelGame: () => command('cancel_game_operation', undefined),
  stopGame: (id: string) => command('stop_game', { id }),
  javaRuntimes: () => command('java_runtimes', undefined),
  inspectJava: (path: string) => command('inspect_java', { path }),
  isDesktop: isTauri(),
  bootstrap: () => command('bootstrap', undefined),
  saveSettings: (request: SaveSettingsRequest) => command('save_settings', { request }),
  librarySnapshot: () => libraryOperation(() => command('library_snapshot', undefined)),
  createInstance: (request: CreateInstance) =>
    libraryOperation(() => command('create_instance', { request })),
  updateInstance: (request: UpdateInstance) =>
    libraryOperation(() => command('update_instance', { request })),
  duplicateInstance: (request: DuplicateInstance) =>
    libraryOperation(() => command('duplicate_instance', { request })),
  deleteInstance: (request: DeleteInstance) =>
    libraryOperation(() => command('delete_instance', { request })),
  saveCollection: (request: SaveCollection) =>
    libraryOperation(() => command('save_collection', { request })),
  deleteCollection: (request: RecordRequest) =>
    libraryOperation(() => command('delete_collection', { request })),
  openLibraryFolder: (request: OpenFolder) => command('open_library_folder', { request }),
  setFavorite: (request: SetFavorite) =>
    libraryOperation(() => command('set_instance_favorite', { request })),
  shortcutPlan: (request: RecordRequest) => command('instance_shortcut_plan', { request }),
  instanceIcon: (id: string) => libraryOperation(() => command('instance_icon', { id })),
  instanceSummary: (id: string) => libraryOperation(() => command('instance_summary', { id })),
  instanceIconCatalog: () => command('instance_icon_catalog', undefined),
  setInstanceIcon: (request: RecordRequest, choice: string) =>
    libraryOperation(() => command('set_instance_icon', { request, choice })),
  pickInstanceIcon: (request: RecordRequest) => command('pick_instance_icon', { request }),
};

// Read-only, clearly labelled preview. Never simulates persistence or a successful install.
export function browserPreview(): Bootstrap {
  return {
    profiles: {
      profiles: [
        {
          id: '00000000-0000-4000-8000-000000000001',
          nickname: 'SporiumLocal',
          offlineUuid: '00000000-0000-3000-8000-000000000001',
        },
      ],
      activeProfileId: '00000000-0000-4000-8000-000000000001',
      revision: 0,
    },
    info: {
      name: 'Sporium',
      version: '0.1.0',
      platform: 'browser',
      databaseSchema: 3,
      dataDirectory: '',
    },
    settings: {
      revision: 0,
      values: {
        schemaVersion: 1,
        locale: 'ru-RU',
        motion: 'system',
        uiScale: 100,
        versionVisibility: {
          releases: true,
          snapshots: false,
          beta: false,
          alpha: false,
          other: false,
        },
        customJavaPath: null,
        localNickname: 'SporiumLocal',
        nicknameSkins: true,
        downloadConcurrency: 6,
        cacheLimitMb: 0,
      },
    },
  };
}
