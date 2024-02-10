import { Link, createLazyFileRoute } from '@tanstack/react-router';
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

export const Route = createLazyFileRoute('/login')({
    component: Login,
})

function Login() {
    return (
        <div className="flex flex-col w-full h-full min-h-screen justify-center items-center">
            <Card className="max-w-lg w-full bg-slate-900 border-slate-600 p-2">
                <CardHeader>
                    <CardTitle className="text-center text-3xl">Login</CardTitle>
                </CardHeader>
                <CardContent className="gap-4 flex flex-col items-center justify-center space-y-2">
                    <div className="grid w-full max-w-sm items-center gap-1.5">
                        <Label className="text-md" htmlFor="email">Username</Label>
                        <Input type="email" id="email" placeholder="Email" />
                    </div>
                    <div className="grid w-full max-w-sm items-center gap-1.5">
                        <Label className="text-md" htmlFor="password">Password</Label>
                        <Input type="password" id="password" placeholder="Password" />
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

        </div>
    )
}