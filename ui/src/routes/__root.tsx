import { createRootRoute, Link, Outlet, useRouterState } from '@tanstack/react-router'
import { TanStackRouterDevtools } from '@tanstack/router-devtools'
import { useEffect } from 'react'

export const Route = createRootRoute({
  component: () => {
    const routerState = useRouterState()

    useEffect(() => {
      console.log(routerState.location)
    }, [routerState.location])

    return (
      <div className="w-full h-full circle-bg min-h-screen text-foreground">
        <div className="p-2 flex gap-2 fixed">
          <Link to="/" className="[&.active]:font-bold">
            Home
          </Link>{' '}
          <Link to="/about" className="[&.active]:font-bold">
            About
          </Link>
        </div>
        <Outlet />
        <TanStackRouterDevtools />
      </div>
    )
  },
})