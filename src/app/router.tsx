import { createHashRouter, Navigate } from 'react-router-dom';
import { Shell } from './Shell';
import { RouteFailure } from './RouteFailure';
import { HomePage } from '../pages/Home';
import { LibraryPage } from '../pages/Library';
import { DownloadsPage, NotFoundPage } from '../pages/StaticPages';
import { CatalogPage } from '../content/CatalogPage';
import { ContentProvider } from '../content/ContentProvider';
import { FoldersPage } from '../pages/Folders';
import { InstancePage } from '../pages/Instance';
import { LibraryProvider } from '../library/LibraryProvider';
import { GameProvider } from '../game/GameProvider';
import { SettingsPage } from '../pages/Settings';

export const router = createHashRouter([
  {
    element: (
      <LibraryProvider>
        <GameProvider>
          <ContentProvider>
            <Shell />
          </ContentProvider>
        </GameProvider>
      </LibraryProvider>
    ),
    errorElement: <RouteFailure />,
    children: [
      { path: '/', element: <HomePage /> },
      { path: '/catalog', element: <CatalogPage /> },
      { path: '/instances', element: <LibraryPage /> },
      { path: '/instances/:kind', element: <LibraryPage /> },
      { path: '/folders', element: <FoldersPage /> },
      { path: '/folders/:id', element: <FoldersPage /> },
      { path: '/instance/:id', element: <InstancePage /> },
      { path: '/downloads', element: <DownloadsPage /> },
      { path: '/settings', element: <Navigate to="/settings/appearance" replace /> },
      { path: '/settings/:section', element: <SettingsPage /> },
      { path: '*', element: <NotFoundPage /> },
    ],
  },
]);
