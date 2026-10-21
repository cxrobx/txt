import React from 'react';
import { createRoot } from 'react-dom/client';
import App from './App.jsx';
import './style.css';

// Margin's stylesheets key dark mode off data-theme; follow the system.
const dark = matchMedia('(prefers-color-scheme: dark)');
const applyTheme = () => { document.documentElement.dataset.theme = dark.matches ? 'dark' : 'light'; };
applyTheme();
dark.addEventListener('change', applyTheme);

createRoot(document.getElementById('root')).render(<App />);
