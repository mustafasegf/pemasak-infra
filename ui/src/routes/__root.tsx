import AuthNavbar from '@/components/AuthNavbar'
import NavSidebar from '@/components/NavSidebar'
import { createRootRoute, Link, Outlet, useRouterState } from '@tanstack/react-router'
import { TanStackRouterDevtools } from '@tanstack/router-devtools'
import { useEffect } from 'react'

export const Route = createRootRoute({
  component: () => {
    const routerState = useRouterState()

    useEffect(() => {
      console.log(routerState.location)
    }, [routerState.location])

    const isAuthRoute = (
      routerState.location.pathname === "/login"
      || routerState.location.pathname === "/register"
    )

    return (
      <div className="w-full h-full circle-bg min-h-screen text-foreground">
        {isAuthRoute ? (
          <>
            <AuthNavbar />
            <Outlet />
          </>
        ) : (
          <div className="flex w-full h-full">
            <NavSidebar className="w-96" />
            <Outlet />
          </div>
        )}
        <TanStackRouterDevtools />
      </div>
    )
  },
})