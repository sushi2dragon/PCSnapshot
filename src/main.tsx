import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { getCurrentWindow } from '@tauri-apps/api/window'
import './index.css'
import App from './App'
import { ErrorOverlay } from './components/ErrorOverlay'

const isOverlay = getCurrentWindow().label === 'overlay'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {isOverlay ? <ErrorOverlay /> : <App />}
  </StrictMode>,
)
