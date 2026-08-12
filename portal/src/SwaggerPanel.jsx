// Swagger UI view, in its own module so it can be lazy-loaded: the library and
// its stylesheet weigh ~1.3 MB of JS and ~180 kB of CSS, more than half the
// bundle, for a view that is opened rarely.
import { Box } from '@mui/material'
import SwaggerUI from 'swagger-ui-react'
import 'swagger-ui-react/swagger-ui.css'

export default function SwaggerPanel() {
  return (
    <Box sx={{
      flex: 1, overflow: 'auto',
      '& .swagger-ui': {
        fontFamily: '"Figtree", sans-serif',
      },
      '& .swagger-ui .topbar': { display: 'none' },
      '& .swagger-ui .info': { margin: '12px 0' },
      '& .swagger-ui .info .title': {
        fontFamily: '"Syne", sans-serif',
        color: '#e8e6f0',
      },
      '& .swagger-ui .info p, & .swagger-ui .info li': { color: '#8b89a0' },
      '& .swagger-ui .scheme-container': { background: 'transparent', boxShadow: 'none', padding: 0 },
      '& .swagger-ui .opblock-tag': { color: '#e8e6f0', borderColor: 'rgba(255,255,255,0.06)' },
      '& .swagger-ui .opblock': { borderColor: 'rgba(255,255,255,0.06)', background: 'rgba(255,255,255,0.02)' },
      '& .swagger-ui .opblock .opblock-summary-method': { borderRadius: '6px', fontFamily: '"Syne", sans-serif' },
      '& .swagger-ui .opblock .opblock-summary-description': { color: '#8b89a0' },
      '& .swagger-ui .opblock .opblock-summary-path': { color: '#e8e6f0' },
      '& .swagger-ui .opblock-body pre': { background: 'rgba(0,0,0,0.3)', color: '#e8e6f0' },
      '& .swagger-ui .model-box, & .swagger-ui .model': { color: '#8b89a0' },
      '& .swagger-ui table thead tr th': { color: '#8b89a0', borderColor: 'rgba(255,255,255,0.06)' },
      '& .swagger-ui table tbody tr td': { color: '#e8e6f0', borderColor: 'rgba(255,255,255,0.06)' },
      '& .swagger-ui .parameter__name': { color: '#00e5ff' },
      '& .swagger-ui .parameter__type': { color: '#8b89a0' },
      '& .swagger-ui .responses-inner h4, & .swagger-ui .responses-inner h5': { color: '#e8e6f0' },
      '& .swagger-ui .response-col_status': { color: '#00e676' },
      '& .swagger-ui .btn': { borderColor: 'rgba(255,255,255,0.1)', color: '#8b89a0' },
      '& .swagger-ui select': { background: 'rgba(20,20,35,0.8)', color: '#e8e6f0', borderColor: 'rgba(255,255,255,0.1)' },
      '& .swagger-ui input[type=text]': { background: 'rgba(20,20,35,0.8)', color: '#e8e6f0', borderColor: 'rgba(255,255,255,0.1)' },
      '& .swagger-ui .opblock-tag:hover': { color: '#00e5ff' },
      '& .swagger-ui .opblock.opblock-get .opblock-summary-method': { bgcolor: '#00e5ff', color: '#0a0a12' },
      '& .swagger-ui .opblock.opblock-post .opblock-summary-method': { bgcolor: '#ffb800', color: '#0a0a12' },
    }}>
      <SwaggerUI url="/api-docs/openapi.json" docExpansion="list" defaultModelsExpandDepth={-1} />
    </Box>
  )
}
