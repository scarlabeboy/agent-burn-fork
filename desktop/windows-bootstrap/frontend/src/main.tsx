import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App.tsx';
import './styles/base.css';
import './styles/shell.css';

const root = document.getElementById('root');
if (root === null) {
  throw new Error('Missing bootstrap root element');
}

createRoot(root).render(<StrictMode><App /></StrictMode>);
