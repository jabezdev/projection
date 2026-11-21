import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
// Bundled so the operator UI looks the same offline (church PCs often have no internet).
import '@fontsource/barlow/latin-400.css'
import '@fontsource/barlow/latin-500.css'
import '@fontsource/barlow/latin-600.css'
import '@fontsource/barlow/latin-700.css'
import '@fontsource/barlow-condensed/latin-500.css'
import '@fontsource/barlow-condensed/latin-600.css'
import './index.css'
import App from './App.tsx'
import { startDemo } from './api/tauri'

// Browser-only mode: every load starts from the demo service.
startDemo()

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
