# K3s Deployment for Mussubotti

This directory contains Kubernetes manifests for deploying Mussubotti to a k3s cluster.

## Prerequisites

- A running k3s cluster
- `kubectl` configured to communicate with your cluster
- Helm installed on your system
- GitHub repository secrets set up for CI/CD

## Manual Deployment

If you want to deploy manually without using GitHub Actions, follow these steps:

1. Create the namespace:

```sh
kubectl create namespace mussubot
```

2. Create a secret with your sensitive configuration:

```sh
kubectl create secret generic mussubotti-secrets \
  --namespace mussubot \
  --from-literal=DISCORD_TOKEN=your_discord_token \
  --from-literal=GOOGLE_CLIENT_ID=your_google_client_id \
  --from-literal=GOOGLE_CLIENT_SECRET=your_google_client_secret \
  --from-literal=GOOGLE_CALENDAR_ID=your_google_calendar_id
```

3. Edit the `configmap.yaml` with your Discord channel and guild IDs:

```sh
# Open the file in your favorite editor
nano configmap.yaml

# Apply the changes
kubectl apply -f configmap.yaml
```

4. Install Redis using Helm:

```sh
# Add Bitnami repository if you haven't already
helm repo add bitnami https://charts.bitnami.com/bitnami
helm repo update

# Install Redis in the mussubot namespace
helm install redis bitnami/redis \
  --namespace mussubot \
  --set auth.enabled=false \
  --set architecture=standalone \
  --set master.persistence.size=1Gi \
  --set master.service.ports.redis=6379 \
  --set master.resources.limits.memory=128Mi \
  --set master.resources.limits.cpu=250m \
  --set master.resources.requests.memory=64Mi \
  --set master.resources.requests.cpu=100m
```

5. Update the Redis URL in the configmap (if needed):

```sh
kubectl patch configmap mussubotti-config \
  --namespace mussubot \
  --type merge \
  --patch '{"data":{"REDIS_URL":"redis://redis-master:6379"}}'
```

6. Deploy the bot:

```sh
# Replace IMAGE_REPO and IMAGE_TAG with your values
cat deployment.yaml | sed "s|\${IMAGE_REPO}|ghcr.io/yourusername/mussubotti|g" | sed "s|\${IMAGE_TAG}|latest|g" | kubectl apply -f -
```

## Automated Deployment via GitHub Actions

The GitHub Actions workflow in this repository automates the build, test, and deploy process:

1. Ensure you have the following secrets set in your GitHub repository:
   - `KUBE_CONFIG`: Your kubeconfig for the k3s cluster (base64 encoded)
   - `DISCORD_TOKEN`: Your Discord bot token
   - `GOOGLE_CLIENT_ID`: Your Google API client ID
   - `GOOGLE_CLIENT_SECRET`: Your Google API client secret
   - `GOOGLE_CALENDAR_ID`: Your Google Calendar ID

2. Update the CI/CD workflow to create the namespace and deploy Redis via Helm.

3. Push to the main branch to trigger the workflow.

## Troubleshooting

If you encounter issues with your deployment:

1. Check pod status:
   ```sh
   kubectl get pods -n mussubot
   ```

2. View pod logs:
   ```sh
   kubectl logs -n mussubot deployment/mussubotti
   ```

3. Describe the deployment:
   ```sh
   kubectl describe -n mussubot deployment mussubotti
   ```

4. Check Redis service:
   ```sh
   kubectl describe -n mussubot service redis-master
   ```

## Deployment Files Organization

The Kubernetes manifests are now split into separate files for better organization and independent deployment:

- `mussubotti-deployment.yaml`: Deployment manifest for the Discord bot
- `work-hours-deployment.yaml`: Deployment manifest for the work hours web application
- `work-hours-service-ingress.yaml`: Service and Ingress definitions for the work hours application
- `configmap.yaml`: Configuration map shared by both applications
- `secret.yaml`: Secret definitions (see below for how to create them)

Each component has its own GitHub Actions workflow:

- `.github/workflows/mussubotti-deploy.yml`: Builds and deploys the Discord bot
- `.github/workflows/work-hours-deploy.yml`: Builds and deploys the work hours web application

## Deployment Process

The components can now be deployed independently, triggered by changes to their respective files. You can also manually trigger deployments using the GitHub Actions web interface. 