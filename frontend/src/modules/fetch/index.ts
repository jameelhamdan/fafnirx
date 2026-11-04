import { defineSourceKind, FetchQueueCard, msg, type FrontendModule } from '@/sdk'
import { NO_WEB, webBody, webLoad, webProblem, type Web } from './web'
import { WebForm } from './web-form'

// The fetch module has no viewer: what it adds to the web app is its kind of source (offered by the Sources page when the module
// is up) and its section of the Processing page.
export default {
  queue: FetchQueueCard,
  sources: [
    defineSourceKind<Web>({
      id: 'web',
      label: msg('Web'),
      intro: msg('The master downloads these files on a schedule and keeps its own copy. A feed brings new files by itself.'),
      initial: NO_WEB,
      form: WebForm,
      body: webBody,
      load: webLoad,
      problem: webProblem,
    }),
  ],
} satisfies FrontendModule
