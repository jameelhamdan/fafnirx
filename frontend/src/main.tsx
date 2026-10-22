import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ThemeProvider } from 'next-themes'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import { I18nProvider } from './lib/i18n-provider.tsx'
import App from './App.tsx'
import { Toaster } from './components/ui/sonner.tsx'
import { TooltipProvider } from './components/ui/tooltip.tsx'
import './index.css'

const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 15_000, retry: 1, refetchOnWindowFocus: true } },
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem disableTransitionOnChange>
      <I18nProvider>
      <QueryClientProvider client={queryClient}>
        <TooltipProvider delayDuration={200}>
          <BrowserRouter>
            <App />
          </BrowserRouter>
          <Toaster richColors closeButton />
        </TooltipProvider>
      </QueryClientProvider>
    </I18nProvider>
    </ThemeProvider>
  </StrictMode>,
)
