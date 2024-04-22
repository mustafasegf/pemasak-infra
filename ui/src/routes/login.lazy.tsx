import { Link, createLazyFileRoute, useRouter } from '@tanstack/react-router';
import {
    Card,
    CardContent,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { useForm } from 'react-hook-form';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { ExclamationTriangleIcon } from '@radix-ui/react-icons';
import { useState } from 'react';

export const Route = createLazyFileRoute('/login')({
    component: Login,
})

function Login() {
    const { register, handleSubmit } = useForm()
    const router = useRouter()

    const [error, setError] = useState({ message: "", error_type: "" })

    async function submitHandler(data: any) {
        const request = await fetch(`${import.meta.env.VITE_API_URL}/login`, {
            method: "POST",
            headers: {
                "Content-Type": "application/json"
            },
            body: JSON.stringify({
                username: data.username,
                password: data.password,
            })
        })
        console.log(request.statusText)

        if (request.status >= 400) {
            const data = await request.json()
            setError(data)
        } else {
            router.navigate({ from: "/login", to: "/" })
        }
    }

    return (
        <form onSubmit={handleSubmit(submitHandler)} className="flex flex-col w-full h-full min-h-screen justify-center items-center space-y-8">
            {error.message && (
                <Alert variant="default" className="max-w-lg w-full border-red-400 text-red-400">
                    <ExclamationTriangleIcon className="h-5 w-5 mt-0.5 !text-red-400" />
                    <AlertTitle className="text-lg font-semibold">
                        Login Failed
                    </AlertTitle>
                    <AlertDescription>
                        {error.message}
                    </AlertDescription>
                </Alert>
            )}
            <Card className="max-w-lg w-full bg-slate-900 border-slate-600 p-2">
                <CardHeader>
                    <CardTitle className="text-center text-3xl">Login</CardTitle>
                </CardHeader>
                <CardContent className="gap-4 flex flex-col items-center justify-center space-y-2">
                    <div className="grid w-full max-w-sm items-center gap-1.5">
                        <Label className="text-md" htmlFor="username">Username</Label>
                        <Input {...register("username")} id="username" placeholder="username" />
                    </div>
                    <div className="grid w-full max-w-sm items-center gap-1.5">
                        <Label className="text-md" htmlFor="password">Password</Label>
                        <Input type="password" placeholder="password" id="password" {...register("password")} />
                    </div>
                </CardContent>
                <CardFooter className="flex flex-col items-center justify-center space-y-4 pt-4">
                    <Button size="lg" variant="default" className="w-2/3 text-foreground">
                        Login
                    </Button>
                    <div className="text-center">
                        <p>
                            Don't have an account?
                        </p>
                        <Link to="/register">
                            <Button variant="link" size="lg" className="text-base">
                                Register Here
                            </Button>
                        </Link>
                    </div>
                </CardFooter>
            </Card>
        </form>
    )
}