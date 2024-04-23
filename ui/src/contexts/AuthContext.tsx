import { useNavigate, useRouterState } from "@tanstack/react-router";
import { FC, ReactElement, ReactNode, createContext, useContext, useEffect, useState } from "react";

export const AuthContext = createContext({
    user: {
        id: "",
        username: "",
        name: "",
    },
    authenticated: false,
    handlers: {
        login: (username: string, password: string) => {},
        refreshAuthState: () => { }
    }
})

interface AuthProviderProps {
    children: ReactNode
}

export function useAuth() {
    return useContext(AuthContext)
}

export default function AuthProvider({ children }: AuthProviderProps): ReactElement<FC> {
    const [auth, setAuth] = useState({
        user: {
            id: "",
            username: "",
            name: "",
        },
        authenticated: false,
        handlers: {
            login: (username: string, password: string) => { },
            refreshAuthState: () => { }
        }
    })

    const navigate = useNavigate()
    const router = useRouterState()

    async function login(username: string, password: string) {
        const request = await fetch(`${import.meta.env.VITE_API_URL}/login`, {
            method: "POST",
            credentials: "include",
            headers: {
                "Content-Type": "application/json"
            },
            body: JSON.stringify({
                username: username,
                password: password,
            })
        })

        if (request.status >= 400) {
            const data = await request.json()
            throw data
        }
        await refreshAuthState()
        navigate({ from: router.location.pathname, to: "/" })
    }

    async function refreshAuthState() {
        try {
            const data = await fetch(`${import.meta.env.VITE_API_URL}/validate`, {
                credentials: "include"
            })
                .then((res) => {
                    if (!res.ok || res.status >= 400) {
                        throw Error("Failed to validate user state")
                    }

                    return res.json()
                })
            
            setAuth({
                ...auth,
                user: {
                    id: data.id,
                    username: data.username,
                    name: data.name,
                },
                authenticated: true,
            })
        } catch (e) {
            setAuth({
                ...auth,
                user: {
                    id: "",
                    username: "",
                    name: "",
                },
                authenticated: false,
            })
        }
    }

    useEffect(() => {
        refreshAuthState()
        const interval = setInterval(refreshAuthState, 5 * 60 * 1000)

        return () => clearInterval(interval)
    }, [])

    return (
        <AuthContext.Provider 
            value={{
                ...auth,
                handlers: {
                    login,
                    refreshAuthState,
                }
            }}
        >
            {children}
        </AuthContext.Provider>
    )
}