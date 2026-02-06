// import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './styles/index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  // Note: StrictMode disabled temporarily to fix double-render issue
  <App />,
)
