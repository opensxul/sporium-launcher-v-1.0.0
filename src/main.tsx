import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { RouterProvider } from 'react-router-dom';
import { Foundation } from './app/Foundation';
import { router } from './app/router';
import './styles/tokens.css';
import './styles/app.css';
import './styles/instances.css';
import './styles/game.css';
import './styles/content.css';
import './styles/projects.css';
import './styles/polish.css';
import './styles/branding.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Foundation>
      <RouterProvider router={router} />
    </Foundation>
  </StrictMode>,
);
